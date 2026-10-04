//! Tiến độ + nhật ký của những việc chạy lâu (xuất báo cáo kỳ thuế…).
//!
//! Việc lâu nhất là xuất báo cáo: tự tải XML qua HTTP cho từng hóa đơn rồi render
//! PDF ~200ms/hóa đơn — để im hàng chục giây thì người dùng chỉ biết máy treo.
//!
//! **Tại sao lại hỏi (poll) chứ không bắn sự kiện:** app chạy ở hai dạng — cửa
//! sổ Tauri và web server mở bằng trình duyệt — mà `app.emit()` không tới được
//! trình duyệt. Nên backend giữ tiến độ trong RAM, frontend gọi
//! [`get_job_progress`] mỗi vài trăm mili giây. Hai dạng chạy giống hệt nhau.
//!
//! Chỉ một việc tại một thời điểm (giao diện chặn nút khi đang bận); việc mới
//! ghi đè việc cũ. `id` do frontend sinh để nó chỉ đọc đúng việc của mình, không
//! nhặt nhầm việc của tab khác.

use std::sync::Mutex;
use tauri::State;

/// Số dòng nhật ký giữ lại. Một lượt chạy bình thường không tới gần, nhưng phải
/// có trần để việc lỗi lặp vô hạn không nuốt hết RAM.
const MAX_LOG: usize = 2_000;

#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct JobProgress {
    /// UUID do frontend sinh — lệch là không trả về (tránh đọc nhầm việc khác).
    pub(crate) id: String,
    /// Tên việc, hiện trên đầu hộp thoại.
    pub(crate) label: String,
    /// Việc đang làm ngay lúc này (1 dòng).
    pub(crate) step: String,
    /// Đã xong / tổng số việc đã biết. `total = 0` = chưa đếm được (mới bắt đầu)
    /// → progressbar chạy vòng vô hạn thay vì hiện phần trăm bịa.
    pub(crate) done: usize,
    pub(crate) total: usize,
    /// Đã kết thúc — frontend ngừng hỏi.
    pub(crate) finished: bool,
    pub(crate) error: Option<String>,
    /// Nhật ký, cũ nhất trước. Sắp xếp mới nhất ở cuối (khung log tự cuộn xuống).
    pub(crate) log: Vec<String>,
}

/// Việc duy nhất đang chạy. Mutex chỉ ôm thao tác ghi rất ngắn (không bao giờ
/// ôm qua một điểm `await`); khóa bị thread khác làm chết thì vẫn đọc tiếp được.
static CURRENT: Mutex<Option<JobProgress>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Option<JobProgress>) -> T) -> T {
    let mut guard = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Bắt đầu một việc mới (ghi đè việc cũ nếu có).
pub(crate) fn begin(id: &str, label: &str) {
    with(|c| {
        *c = Some(JobProgress {
            id: id.to_string(),
            label: label.to_string(),
            step: "Chuẩn bị…".to_string(),
            ..Default::default()
        });
    });
}

/// Cộng thêm số việc của chặng sắp chạy.
///
/// Tiến độ = đã xong / tổng **đã biết**, mà tổng thì cộng dần theo từng chặng
/// (XML tải trước, PDF đếm được khi đã dò danh sách, ZIP tính sau) — nên tỉ lệ
/// luôn tăng dần thay vì bật cục khi chặng mới bắt đầu.
pub(crate) fn plan(n: usize) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            j.total += n;
        }
    });
}

/// Chỉ đổi dòng "đang làm" (không ghi nhật ký) — dùng để chỉ hóa đơn đang tải.
pub(crate) fn step(text: impl Into<String>) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            j.step = text.into();
        }
    });
}

/// Ghi một dòng nhật ký vào việc hiện tại (không có việc → bỏ qua vô hại).
fn push(j: &mut JobProgress, line: String) {
    j.step = line;
    j.log.push(j.step.clone());
    // Trần bộ nhớ: một việc lỗi lặp vô hạn không được nuốt hết RAM.
    if j.log.len() > MAX_LOG {
        let over = j.log.len() - MAX_LOG;
        j.log.drain(0..over);
    }
}

/// Ghi một dòng nhật ký (đồng thời làm dòng "đang làm").
pub(crate) fn log(line: impl Into<String>) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            push(j, line.into());
        }
    });
}

/// Xong **một việc** trong tổng: +1 đã làm và ghi nhật ký kết quả việc đó.
pub(crate) fn tick(line: impl Into<String>) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            j.done += 1;
            push(j, line.into());
        }
    });
}

/// Xong **nhiều việc** một lượt — cho chặng nhanh (giải mã file có sẵn) chỉ cần
/// một dòng tổng kết, không spam từng file.
pub(crate) fn advance_by(n: usize) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            j.done += n;
        }
    });
}

/// Kết thúc — gọi ở MỌI đường ra (kể cả lỗi) để hộp thoại không hỏi tới đời.
pub(crate) fn finish(error: Option<String>) {
    with(|c| {
        if let Some(j) = c.as_mut() {
            j.finished = true;
            match &error {
                Some(e) => j.log.push(format!("✗ {e}")),
                None => j.log.push(format!("✓ Hoàn tất — {} mục", j.done)),
            }
            j.error = error;
        }
    });
}

/// Ảnh chụp để frontend đọc. Trả `None` nếu việc này đã bị thay bằng việc khác.
pub(crate) fn snapshot(id: &str) -> Option<JobProgress> {
    with(|c| c.clone().filter(|j| j.id == id))
}

/// Trả tiến độ + nhật ký của việc frontend đang chờ.
///
/// Lệnh **đồng bộ về bản chất** (chỉ đọc RAM) nhưng phải `async` vì trình điều
/// phối web gọi lệnh mọi nơi đều `await`.
///
/// `_state` không dùng tới nhưng vẫn phải nhận: trình điều phối web ghép lệnh
/// theo chỗ, lệnh nào cũng được chèn State vào đầu ([`crate::web::mx`]).
#[tauri::command]
pub(crate) async fn get_job_progress(
    _state: State<'_, crate::models::AppState>,
    job_id: String,
) -> Result<String, String> {
    Ok(serde_json::to_string(&snapshot(&job_id)).unwrap_or_else(|_| "null".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiến độ là state toàn cục (một việc tại một thời điểm) nên gom mọi kiểm
    /// chứng vào MỘT test: các test chạy song song sẽ đua nhau trên ô chung.
    #[test]
    fn job_progress_counts_work_logs_every_step_and_only_answers_for_its_own_id() {
        begin("job-a", "Xuất báo cáo kỳ thuế");
        // Chưa đếm được gì → total = 0 để progressbar chạy vòng thay vì hiện % sai.
        assert_eq!(snapshot("job-a").expect("có việc").total, 0);

        // `plan` chỉ dựng TỔNG việc của chặng; `done` chỉ tăng khi việc đã xong.
        plan(2);
        tick("Tải XML 1/2 — ok");
        tick("Tải XML 2/2 — ok");
        assert_eq!(snapshot("job-a").expect("có việc").done, 2);
        // Chưa xong thì không được đánh dấu finished (frontend còn phải hỏi).
        assert!(!snapshot("job-a").expect("có việc").finished);

        log("Nén ZIP");
        let job = snapshot("job-a").expect("có việc");
        assert_eq!(job.label, "Xuất báo cáo kỳ thuế");
        assert_eq!(job.step, "Nén ZIP", "dòng mới nhất cũng là dòng đang làm");
        assert_eq!(
            job.log,
            vec!["Tải XML 1/2 — ok", "Tải XML 2/2 — ok", "Nén ZIP"]
        );

        // Việc khác không đọc được (frontend không được thấy tiến độ của tab khác).
        assert!(snapshot("job-b").is_none(), "id lạ không được trả về");

        finish(None);
        let job = snapshot("job-a").expect("có việc");
        assert!(job.finished);
        assert!(job.error.is_none());
        assert_eq!(job.log.last().expect("có dòng kết"), "✓ Hoàn tất — 2 mục");

        // Việc mới ghi đè việc cũ.
        begin("job-b", "Việc khác");
        assert!(snapshot("job-a").is_none(), "việc cũ đã bị thay");
        finish(Some("hỏng".into()));
        let job = snapshot("job-b").expect("có việc");
        assert_eq!(job.error.as_deref(), Some("hỏng"));
        assert_eq!(
            job.log.last().expect("có dòng lỗi"),
            "✗ hỏng",
            "lỗi phải nằm ở dòng cuối nhật ký để người dùng đọc thấy ngay"
        );
    }
}
