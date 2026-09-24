#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Một lần "lấy" hàng ra khỏi 1 lô theo FIFO.
#[derive(Debug, PartialEq)]
pub(crate) struct FifoTake {
    pub(crate) lot_id: i64,
    pub(crate) new_qty: f64,
    pub(crate) depleted: bool,
}

/// Phân bổ số lượng cần xuất `qty` lên danh sách lô `(id, quantity, unit_cost)`
/// theo FIFO (lô nhập trước xuất trước):
/// - Đủ hàng: trả `Ok((giá vốn, danh sách cập nhật từng lô))`.
/// - Thiếu hàng: trả `Err(số lượng còn thiếu)` (không xuất gì thêm).
pub(crate) fn allocate_fifo(
    lots: &[(i64, f64, f64)],
    qty: f64,
) -> Result<(f64, Vec<FifoTake>), f64> {
    let mut remaining = qty;
    let mut cogs = 0.0;
    let mut takes = Vec::new();
    for &(lot_id, lot_qty, lot_cost) in lots {
        if remaining <= 1e-9 {
            break;
        }
        let take = remaining.min(lot_qty);
        cogs += take * lot_cost;
        remaining -= take;
        let new_qty = lot_qty - take;
        takes.push(FifoTake {
            lot_id,
            new_qty,
            depleted: new_qty <= 1e-9,
        });
    }
    if remaining > 1e-9 {
        return Err(remaining);
    }
    Ok((cogs, takes))
}

/// Kết quả lưu 1 phiếu nhập kho.
#[derive(Debug)]
pub(crate) struct InboundResult {
    pub(crate) entries: usize,
    /// Tổng giá trị hàng nhập kho (chưa gồm thuế khi khấu trừ GTGT).
    pub(crate) total: f64,
    /// Tổng thuế GTGT đầu vào được khấu trừ (0 nếu không khấu trừ).
    pub(crate) vat: f64,
    /// Số phiếu chi (PC) tự tạo khi bật "Trả tiền ngay" (rỗng nếu không tạo).
    pub(crate) pc_no: String,
}

/// Lõi nhập kho (không phụ thuộc Tauri State → test trực tiếp):
/// tạo journal_entry (PN) + stock_lot (FIFO) trong 1 transaction.
/// Theo TT 88/2021/TT-BTC (mẫu 03-VT), Phiếu nhập kho áp dụng cho MỌI trường
/// hợp hàng vào kho: mua ngoài (purchase), tự sản xuất / thuê gia công
/// (production), nhập khác (other — thừa kiểm kê, điều chỉnh...).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_inbound_core(
    pool: &SqlitePool,
    posting_date: &str,
    voucher_no: &str,
    description: &str,
    supplier_code: &str,
    warehouse_code: &str,
    unit_code: &str,
    items: &[InboundItemInput],
    note: &str,
    inbound_type: &str,
    reference_no: &str,
    vat_rate: f64,
    debit_account: &str,
    credit_account: &str,
    pay_now: bool,
    adjust_dir: &str,
) -> Result<InboundResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Đầu phiếu nhập (inbound_voucher) — id này là điểm neo để hóa đơn chính
    // thức liên kết chặt với phiếu nhập. Tạo TRƯỚC các dòng bút toán để
    // gắn inbound_voucher_id ngay khi ghi.
    sqlx::query(
        "INSERT INTO inbound_voucher
            (voucher_no, posting_date, supplier_code, description, reference_no,
             inbound_type, warehouse_code, unit_code, vat_rate, source, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'manual', '', datetime('now'))
         ON CONFLICT(voucher_no) DO NOTHING",
    )
    .bind(voucher_no)
    .bind(posting_date)
    .bind(supplier_code)
    .bind(description)
    .bind(reference_no)
    .bind(inbound_type)
    .bind(warehouse_code)
    .bind(unit_code)
    .bind(vat_rate)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let inbound_header_id: (i64,) =
        sqlx::query_as("SELECT id FROM inbound_voucher WHERE voucher_no = ?")
            .bind(voucher_no)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

    // TK mặc định theo loại nhập (rỗng → mặc định):
    //   purchase  → Nợ 152 (hàng hóa) / Có 331 (phải trả người bán)
    //   production→ Nợ 155 (thành phẩm) / Có 154 (chi phí SXKD dở dang)
    //   other     → Nợ 152 / Có 154 (người dùng tự chọn nếu cần)
    let debit = if debit_account.trim().is_empty() {
        if inbound_type == "production" {
            "155"
        } else {
            "152"
        }
    } else {
        debit_account
    };
    let credit = if credit_account.trim().is_empty() {
        if inbound_type == "production" {
            "154"
        } else {
            "331"
        }
    } else {
        credit_account
    };

    let mut total = 0.0;
    let mut vat_total = 0.0;
    // Số chứng từ gốc (hóa đơn / lệnh nhập kho — cột "Theo..." của mẫu 03-VT)
    // chuyển vào cột ghi chú của bút toán để truy vết được chứng từ gốc.
    let note_full = if reference_no.trim().is_empty() {
        note.to_string()
    } else if note.trim().is_empty() {
        format!("Theo chứng từ: {}", reference_no.trim())
    } else {
        format!("Theo chứng từ: {} · {}", reference_no.trim(), note)
    };

    for item in items {
        // Giá trị nhập kho = Thành tiền − Tiền CK (chiết khấu thương mại).
        // VAT đầu vào tính trên giá sau chiết khấu (CK được trừ vào giá tính thuế).
        let gross = round2(item.quantity * item.unit_price);
        // Tiền CK không thể vượt quá thành tiền (tránh giá trị âm).
        let discount = round2(item.discount.clamp(0.0, gross));
        let net = round2(gross - discount);
        // Thuế GTGT đầu vào — chỉ hạch toán khi hộ được khấu trừ (vat_rate > 0).
        let vat = if vat_rate > 0.0 {
            round2(net * vat_rate)
        } else {
            0.0
        };

        // ─── Điều chỉnh GIẢM hóa đơn mua (bên bán trừ bớt): hàng trả lại NCC ───
        // Trừ FIFO khỏi lô (giảm tồn thực tế, giống xuất kho) + ghi sổ đảo ngược:
        //   Nợ TK đối ứng (mặc định 331 — giảm phải trả NCC) / Có TK hàng (mặc
        //   định 152 — giảm giá trị hàng mua). Nếu khấu trừ GTGT, thêm bút toán
        //   Nợ 331 / Có 133 (giảm thuế GTGT đầu vào). Đánh dấu adjust_code =
        //   'GiamCP' để báo cáo và màn Tồn kho hiểu đây là dòng điều chỉnh.
        if inbound_type == "adjust" && adjust_dir == "down" {
            let wh = resolve_warehouse(&mut tx, warehouse_code, &item.product_code).await?;
            let product_id = resolve_product(&mut tx, &item.product_code).await?.id;
            let lots: Vec<(i64, f64, f64)> = sqlx::query_as(
                "SELECT id, quantity, unit_cost FROM stock_lot
                 WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
                 ORDER BY received_at ASC, id ASC",
            )
            .bind(product_id)
            .bind(wh)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            let (_, takes) = allocate_fifo(&lots, item.quantity).map_err(|short| {
                format!(
                    "Không đủ tồn kho để điều chỉnh giảm '{}': thiếu {:.2} (còn thiếu sau khi trừ hết các lô)",
                    item.product_code, short
                )
            })?;
            for t in takes {
                sqlx::query!(
                    "UPDATE stock_lot SET quantity = ?, depleted = ? WHERE id = ?",
                    t.new_qty,
                    t.depleted,
                    t.lot_id
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            }
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "PN",
                description,
                &item.product_code,
                supplier_code,
                "",
                item.quantity,
                item.unit_price,
                net,
                credit,
                debit,
                "",
                0.0,
                0.0,
                unit_code,
                "GiamCP",
                &note_full,
            )
            .await?;
            if vat > 0.0 {
                insert_journal_entry(
                    &mut tx,
                    posting_date,
                    voucher_no,
                    "PN",
                    description,
                    "",
                    supplier_code,
                    "",
                    0.0,
                    0.0,
                    vat,
                    credit,
                    "133",
                    "",
                    0.0,
                    0.0,
                    unit_code,
                    "GiamCP",
                    &note_full,
                )
                .await?;
            }
            total += net;
            vat_total += vat;
            continue;
        }

        // ─── Nhập kho thường (mua ngoài / tự sản xuất / điều chỉnh tăng / khác) ───
        let wh_id = resolve_warehouse(&mut tx, warehouse_code, &item.product_code).await?;
        let product_id = resolve_product(&mut tx, &item.product_code).await?.id;
        // Đơn giá thực tế sau chiết khấu — dùng cho giá vốn FIFO và ghi sổ.
        let unit_cost = round2(net / item.quantity);
        // tạo stock_lot FIFO — giá vốn = đơn giá thực tế sau chiết khấu (chưa thuế khi khấu trừ)
        sqlx::query!(
            "INSERT INTO stock_lot (product_id, warehouse_id, quantity, unit_cost, received_at, depleted)
             VALUES (?, ?, ?, ?, ?, 0)",
            product_id,
            wh_id,
            item.quantity,
            unit_cost,
            posting_date
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        // ghi sổ nhập — giá trị nhập kho = Thành tiền − Tiền CK (chưa gồm thuế khi khấu trừ)
        insert_journal_entry(
            &mut tx,
            posting_date,
            voucher_no,
            "PN",
            description,
            &item.product_code,
            supplier_code,
            "",
            item.quantity,
            unit_cost,
            net,
            debit,
            credit,
            "",
            0.0,
            0.0,
            unit_code,
            "",
            &note_full,
        )
        .await?;
        // Khấu trừ GTGT: bút toán bổ sung Nợ 133 (thuế được khấu trừ) / Có credit.
        // Số lượng 0 để không ảnh hưởng tồn kho / báo cáo số lượng.
        if vat > 0.0 {
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "PN",
                description,
                "",
                supplier_code,
                "",
                0.0,
                0.0,
                vat,
                "133",
                credit,
                "",
                0.0,
                0.0,
                unit_code,
                "",
                &note_full,
            )
            .await?;
        }
        total += net;
        vat_total += vat;
    }

    // Công tắc "Trả tiền ngay" (mặc định BẬT): mua hàng ngoài có nhà cung cấp → tự tạo
    // phiếu chi (PC) thanh toán trong cùng transaction: Nợ TK đối ứng của PNK (mặc định
    // 331 — phải trả người bán) / Có 111 tiền mặt. Số tiền = Giá trị nhập kho + Thuế
    // GTGT đầu vào (nếu hộ được khấu trừ). Không tạo khi TK Có PNK đã là tiền mặt /
    // ngân hàng (hàng đã trả ngay ngay trên PNK) hoặc chưa chọn nhà cung cấp.
    let mut pc_no = String::new();
    let credit_is_cash = credit == "111" || credit == "112";
    if pay_now && inbound_type == "purchase" && !supplier_code.trim().is_empty() && !credit_is_cash
    {
        let pay = round2(total + vat_total);
        if pay > 0.0 {
            pc_no = next_pc_no(&mut tx).await?;
            let desc = if description.trim().is_empty() {
                "Trả tiền mua hàng".to_string()
            } else {
                format!("Trả tiền mua hàng — {}", description.trim())
            };
            // Liên kết phiếu chi với PNK qua số chứng từ gốc + mã nhà cung cấp.
            let pc_note = format!("Theo chứng từ: {}", voucher_no);
            insert_journal_entry(
                &mut tx,
                posting_date,
                &pc_no,
                "PC",
                &desc,
                "",
                supplier_code,
                "",
                1.0,
                pay,
                pay,
                credit,
                "111",
                "",
                0.0,
                0.0,
                unit_code,
                "",
                &pc_note,
            )
            .await?;
        }
    }

    // Gắn id đầu phiếu vào các dòng bút toán của chính phiếu này + chốt tổng.
    // (Lọc theo voucher_no + entry_type để không đụng phiếu khác trùng số.)
    sqlx::query(
        "UPDATE journal_entry SET inbound_voucher_id = ?
          WHERE voucher_no = ? AND entry_type = 'PN'",
    )
    .bind(inbound_header_id.0)
    .bind(voucher_no)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE inbound_voucher SET total = ?, vat_amount = ?, reference_no = ?,
                posting_date = ?, supplier_code = ?, description = ?, unit_code = ?
          WHERE id = ?",
    )
    .bind(total)
    .bind(vat_total)
    .bind(reference_no)
    .bind(posting_date)
    .bind(supplier_code)
    .bind(description)
    .bind(unit_code)
    .bind(inbound_header_id.0)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(InboundResult {
        entries: items.len(),
        total,
        vat: vat_total,
        pc_no,
    })
}

/// Số phiếu chi (PC) tiếp theo: lấy số lớn nhất đang có + 1 (PC001, PC002…).
/// Khớp với quy ước gợi ý số phiếu ở màn Phiếu thu/chi.
async fn next_pc_no(tx: &mut SqliteTransaction<'_>) -> Result<String, String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT voucher_no FROM journal_entry WHERE entry_type = 'PC'")
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    let max = rows
        .iter()
        .filter_map(|(v,)| {
            let digits: String = v
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            digits.parse::<i64>().ok()
        })
        .max()
        .unwrap_or(0);
    Ok(format!("PC{:03}", max + 1))
}

/// Kết quả lưu 1 phiếu xuất kho.
#[derive(Debug)]
pub(crate) struct OutboundResult {
    pub(crate) entries: usize,
    pub(crate) revenue: f64,
    pub(crate) cogs: f64,
    /// Số phiếu thu (PT) tự tạo khi bật "Thu tiền ngay" (rỗng nếu không tạo).
    pub(crate) pt_no: String,
    /// Số hóa đơn bán hàng tự lập kèm (tab "Hóa đơn") — rỗng nếu không lập.
    pub(crate) invoice_no: String,
    /// "draft" | "official" — trạng thái hóa đơn vừa lập kèm (rỗng nếu không lập).
    pub(crate) invoice_status: String,
}

/// Số phiếu thu (PT) tiếp theo: lấy số lớn nhất đang có + 1 (PT001, PT002…).
/// Khớp với quy ước gợi ý số phiếu ở màn Phiếu thu/chi.
async fn next_pt_no(tx: &mut SqliteTransaction<'_>) -> Result<String, String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT voucher_no FROM journal_entry WHERE entry_type = 'PT'")
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    let max = rows
        .iter()
        .filter_map(|(v,)| {
            let digits: String = v
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            digits.parse::<i64>().ok()
        })
        .max()
        .unwrap_or(0);
    Ok(format!("PT{:03}", max + 1))
}

/// Lõi xuất kho (không phụ thuộc Tauri State → test trực tiếp):
/// xuất FIFO từ stock_lot, ghi doanh thu (PX) kèm giá vốn.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_outbound_core(
    pool: &SqlitePool,
    posting_date: &str,
    voucher_no: &str,
    description: &str,
    customer_code: &str,
    unit_code: &str,
    items: &[OutboundItemInput],
    note: &str,
    receive_now: bool,
    outbound_type: &str,
    adjust_dir: &str,
    invoice: &OutboundInvoiceInput,
) -> Result<OutboundResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut revenue_total = 0.0;
    let mut cogs_total = 0.0;
    // Dòng mặt hàng của hóa đơn lập kèm (chỉ các phiếu thực tăng doanh thu).
    let mut invoice_lines: Vec<(i64, f64, f64, String, f64, f64)> = Vec::new();

    // Điều chỉnh GIẢM hóa đơn bán (khách trả lại / giảm doanh thu): ghi đảo doanh
    // thu và nhập lại hàng về kho, KHÔNG xuất FIFO và không tạo phiếu thu.
    let is_adjust_down = outbound_type == "adjust" && adjust_dir == "down";
    // Hóa đơn bán hàng chỉ lập KÈM cho phiếu thực tăng doanh thu (bán thường /
    // điều chỉnh tăng); phiếu giảm (khách trả lại) chỉ ghi đảo doanh thu → không
    // phát hành hóa đơn mới.
    let is_sale = outbound_type == "sale" || (outbound_type == "adjust" && adjust_dir == "up");
    let make_invoice = is_sale && !invoice.number.trim().is_empty() && !items.is_empty();

    for item in items {
        let product = resolve_product(&mut tx, &item.product_code).await?;

        // Nhóm ngành trên dòng: ưu tiên khai trên dòng, rỗng → nhóm ngành mặc định
        // của sản phẩm (cơ sở tỷ lệ thuế bán ra).
        let industry = if item.industry_code.trim().is_empty() {
            product.industry_code.clone()
        } else {
            item.industry_code.trim().to_string()
        };
        if industry.is_empty() {
            return Err(format!(
                "Sản phẩm '{}' chưa có nhóm ngành — hãy gán nhóm ngành cho sản phẩm (màn Sản phẩm) hoặc chọn nhóm ngành trên dòng",
                item.product_code
            ));
        }

        // tỷ lệ thuế theo nhóm ngành
        let (vat_rate, pit_rate) = {
            let r = sqlx::query!(
                "SELECT vat_rate, pit_rate FROM industry_group WHERE code = ?",
                industry
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| format!("Nhóm ngành '{}' không hợp lệ", industry))?;
            (r.vat_rate, r.pit_rate)
        };

        // ghi sổ bán hàng (doanh thu = giá bán * SL)
        let amount = item.quantity * item.unit_price;

        // Sản phẩm dịch vụ (nhân công...): không theo dõi tồn kho → chỉ ghi doanh thu,
        // không xuất FIFO, không giá vốn.
        if product.is_service {
            // Điều chỉnh giảm dịch vụ: chỉ giảm doanh thu (Nợ 511 / Có 131).
            let (debit, credit, adjust) = if is_adjust_down {
                ("511", "131", "GiamDT")
            } else {
                ("131", "511", "")
            };
            if !is_adjust_down {
                invoice_lines.push((
                    product.id,
                    item.quantity,
                    item.unit_price,
                    industry.clone(),
                    vat_rate,
                    pit_rate,
                ));
            }
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "PX",
                description,
                &item.product_code,
                "",
                customer_code,
                item.quantity,
                item.unit_price,
                amount,
                debit,
                credit,
                &industry,
                vat_rate,
                pit_rate,
                unit_code,
                adjust,
                note,
            )
            .await?;
            revenue_total += amount;
            continue;
        }

        // ─── Điều chỉnh GIẢM hóa đơn bán: khách trả lại / giảm doanh thu ───
        // 1) Giảm doanh thu + giảm phải thu: Nợ 511 / Có 131 (PX, adjust = 'GiamDT').
        // 2) Nhập lại hàng về kho (lô mới, giá vốn theo đơn giá điều chỉnh) + ghi
        //    giảm giá vốn hàng bán: Nợ 152 / Có 632.
        if is_adjust_down {
            let wh_id =
                resolve_warehouse(&mut tx, &item.warehouse_code, &item.product_code).await?;
            sqlx::query!(
                "INSERT INTO stock_lot (product_id, warehouse_id, quantity, unit_cost, received_at, depleted)
                 VALUES (?, ?, ?, ?, ?, 0)",
                product.id,
                wh_id,
                item.quantity,
                item.unit_price,
                posting_date
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "PX",
                description,
                &item.product_code,
                "",
                customer_code,
                item.quantity,
                item.unit_price,
                amount,
                "511",
                "131",
                &industry,
                vat_rate,
                pit_rate,
                unit_code,
                "GiamDT",
                note,
            )
            .await?;
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "PN",
                description,
                &item.product_code,
                "",
                "",
                item.quantity,
                item.unit_price,
                amount,
                "152",
                "632",
                "",
                0.0,
                0.0,
                unit_code,
                "",
                note,
            )
            .await?;
            revenue_total += amount;
            continue;
        }

        // Kho xuất trên dòng: rỗng → kho mặc định của sản phẩm / kho đầu tiên.
        let wh_id = resolve_warehouse(&mut tx, &item.warehouse_code, &item.product_code).await?;

        // Hàng hóa: FIFO lấy các lô chưa xuất hết theo ngày nhập, trong đúng kho đã chọn
        let lots: Vec<(i64, f64, f64)> = sqlx::query_as(
            "SELECT id, quantity, unit_cost FROM stock_lot
             WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
             ORDER BY received_at ASC, id ASC",
        )
        .bind(product.id)
        .bind(wh_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let wh_name = if item.warehouse_code.trim().is_empty() {
            "kho mặc định".to_string()
        } else {
            item.warehouse_code.clone()
        };
        let (cogs, takes) = allocate_fifo(&lots, item.quantity).map_err(|short| {
            format!(
                "Không đủ tồn kho cho '{}' tại {}: thiếu {:.2} (còn thiếu sau khi xuất hết các lô)",
                item.product_code, wh_name, short
            )
        })?;
        for t in takes {
            sqlx::query!(
                "UPDATE stock_lot SET quantity = ?, depleted = ? WHERE id = ?",
                t.new_qty,
                t.depleted,
                t.lot_id
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }

        insert_journal_entry(
            &mut tx,
            posting_date,
            voucher_no,
            "PX",
            description,
            &item.product_code,
            "",
            customer_code,
            item.quantity,
            item.unit_price,
            amount,
            "131",
            "511",
            &industry,
            vat_rate,
            pit_rate,
            unit_code,
            "",
            note,
        )
        .await?;
        invoice_lines.push((
            product.id,
            item.quantity,
            item.unit_price,
            industry.clone(),
            vat_rate,
            pit_rate,
        ));
        revenue_total += amount;
        cogs_total += cogs;
    }

    // Công tắc "Thu tiền ngay" (mặc định BẬT): bán hàng có khách hàng → tự tạo
    // phiếu thu (PT) trong cùng transaction: Nợ 111 tiền mặt / Có 131 phải thu
    // khách hàng. Số tiền = tổng doanh thu của PX. Liên kết qua ghi chú chứa số
    // PX + mã khách hàng. Không tạo khi chưa chọn khách hàng (chưa biết thu của ai)
    // hoặc khi là phiếu điều chỉnh hóa đơn (không thu tiền mới).
    let mut pt_no = String::new();
    if receive_now && outbound_type == "sale" && !customer_code.trim().is_empty() {
        let collect = round2(revenue_total);
        if collect > 0.0 {
            pt_no = next_pt_no(&mut tx).await?;
            let desc = if description.trim().is_empty() {
                "Thu tiền bán hàng".to_string()
            } else {
                format!("Thu tiền bán hàng — {}", description.trim())
            };
            // Liên kết phiếu thu với PX qua số chứng từ gốc + mã khách hàng.
            let pt_note = format!("Theo chứng từ: {}", voucher_no);
            insert_journal_entry(
                &mut tx,
                posting_date,
                &pt_no,
                "PT",
                &desc,
                "",
                "",
                customer_code,
                1.0,
                collect,
                collect,
                "111",
                "131",
                "",
                0.0,
                0.0,
                unit_code,
                "",
                &pt_note,
            )
            .await?;
        }
    }

    // ─── Hóa đơn bán hàng lập KÈM phiếu xuất (tab "Hóa đơn") ───
    // Tự sinh 1 hóa đơn cho phiếu xuất thực tăng doanh thu: số hóa đơn = số phiếu
    // xuất (liên kết 1-1, "không lệch sổ"). Khai Số HĐĐT + Ký hiệu + Ngày ngay trên
    // phiếu → hóa đơn chuyển thành "Đã liên kết HĐĐT"; ngược lại chỉ là hóa đơn
    // nháp (draft), có thể liên kết HĐĐT sau từ tab "Hóa đơn".
    let mut inv_no = String::new();
    let mut inv_status = String::new();
    if make_invoice && !invoice_lines.is_empty() {
        let (customer_name, customer_tax) = if customer_code.trim().is_empty() {
            (String::new(), String::new())
        } else {
            let r = sqlx::query_as::<_, (String, String)>(
                "SELECT name, COALESCE(tax_code, '') FROM customer WHERE code = ?",
            )
            .bind(customer_code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            r.unwrap_or_default()
        };
        inv_no = invoice.number.clone();
        inv_status = if invoice.e_invoice_no.trim().is_empty() {
            "draft".to_string()
        } else {
            "official".to_string()
        };
        let total_value = round2(revenue_total);
        let res = sqlx::query!(
            "INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount,
                                  status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no)
             VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)",
            inv_no,
            posting_date,
            customer_name,
            customer_tax,
            total_value,
            inv_status,
            invoice.e_invoice_no,
            invoice.e_invoice_symbol,
            invoice.e_invoice_date,
            voucher_no
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let inv_id = res.last_insert_rowid();
        // Dòng chi tiết hóa đơn — giữ nguyên nhóm ngành / tỷ lệ thuế đã hạch toán.
        for (pid, qty, price, ind, vat, pit) in &invoice_lines {
            let subtotal = qty * price;
            sqlx::query(
                "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                           industry_code, vat_rate, pit_rate)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(inv_id)
            .bind(pid)
            .bind(qty)
            .bind(price)
            .bind(subtotal)
            .bind(ind)
            .bind(vat)
            .bind(pit)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(OutboundResult {
        entries: items.len(),
        revenue: revenue_total,
        cogs: cogs_total,
        pt_no,
        invoice_no: inv_no,
        invoice_status: inv_status,
    })
}

/// Nhập kho: tạo journal_entry (PN) + stock_lot (FIFO) trong 1 transaction.
/// inbound_type: "purchase" (mua ngoài) | "production" (tự sản xuất, gia công) | "other".
/// vat_rate (%): chỉ được khấu trừ khi bật app_setting 'vat_deduct' (= '1').
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_inbound(
    state: State<'_, AppState>,
    posting_date: String,
    voucher_no: String,
    description: String,
    supplier_code: String,
    warehouse_code: String,
    unit_code: String,
    items: Vec<InboundItemInput>,
    note: String,
    inbound_type: String,
    reference_no: String,
    vat_rate: f64,
    debit_account: String,
    credit_account: String,
    pay_now: bool,
    adjust_dir: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong phiếu nhập".into());
    }
    let inbound_type = if inbound_type.is_empty() {
        "purchase"
    } else {
        &inbound_type
    };
    if inbound_type != "purchase"
        && inbound_type != "production"
        && inbound_type != "other"
        && inbound_type != "adjust"
    {
        return Err("Loại nhập không hợp lệ (purchase | production | other | adjust)".into());
    }
    // Hướng điều chỉnh chỉ có ý nghĩa với loại nhập "adjust":
    //   down = giảm (bên bán trừ bớt — trả lại NCC), up = tăng (bên bán thêm).
    let adjust_dir = if inbound_type == "adjust" {
        if adjust_dir != "down" && adjust_dir != "up" {
            return Err("Hướng điều chỉnh không hợp lệ (down | up)".into());
        }
        adjust_dir.as_str()
    } else {
        ""
    };
    // Công tắc "khấu trừ GTGT đầu vào" (mặc định TẮT) — tắt thì bỏ qua thuế nhập
    // kể cả khi form gửi lên (hộ nộp thuế theo doanh thu không được khấu trừ).
    let pool = state.pool.read().await;
    let vat_deduct: (String,) = sqlx::query_as(
        "SELECT COALESCE((SELECT value FROM app_setting WHERE key = 'vat_deduct'), '0')",
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    let vat_rate = if vat_deduct.0 == "1" { vat_rate } else { 0.0 };

    // TK Nợ/Có (kể cả mặc định theo loại nhập) phải có trong danh mục tài khoản.
    let debit = if debit_account.trim().is_empty() {
        if inbound_type == "production" {
            "155"
        } else {
            "152"
        }
    } else {
        &debit_account
    };
    let credit = if credit_account.trim().is_empty() {
        if inbound_type == "production" {
            "154"
        } else {
            "331"
        }
    } else {
        &credit_account
    };
    let mut checked = vec![debit.to_string(), credit.to_string()];
    if vat_rate > 0.0 {
        checked.push("133".to_string());
    }
    // "Trả tiền ngay": phiếu chi tự tạo ghi Có 111 tiền mặt → TK này cũng phải có
    // trong danh mục (trừ khi PNK đã trả thẳng tiền mặt/ngân hàng — không tạo PC).
    if pay_now
        && inbound_type == "purchase"
        && !supplier_code.trim().is_empty()
        && credit != "111"
        && credit != "112"
    {
        checked.push("111".to_string());
    }
    for code in &checked {
        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM account WHERE code = ?")
            .bind(code)
            .fetch_one(&*pool)
            .await
            .map_err(|e| e.to_string())?;
        if exists.0 == 0 {
            return Err(format!(
                "Tài khoản '{}' chưa có trong danh mục tài khoản — hãy thêm ở màn Tài khoản trước khi nhập kho",
                code
            ));
        }
    }

    let unit_code = if unit_code.is_empty() {
        "HKD".to_string()
    } else {
        unit_code
    };
    let r = save_inbound_core(
        &pool,
        &posting_date,
        &voucher_no,
        &description,
        &supplier_code,
        &warehouse_code,
        &unit_code,
        &items,
        &note,
        inbound_type,
        &reference_no,
        vat_rate,
        debit,
        credit,
        pay_now,
        adjust_dir,
    )
    .await?;
    audit(&state, "save", "inbound", &voucher_no).await;
    Ok(json!({
        "ok": true,
        "entries": r.entries,
        "total": r.total,
        "vat": r.vat,
        "pc_no": r.pc_no,
    })
    .to_string())
}

/// Xuất kho: FIFO từ stock_lot, ghi doanh thu (PX) kèm giá vốn
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_outbound(
    state: State<'_, AppState>,
    posting_date: String,
    voucher_no: String,
    description: String,
    customer_code: String,
    unit_code: String,
    items: Vec<OutboundItemInput>,
    note: String,
    receive_now: bool,
    outbound_type: String,
    adjust_dir: String,
    create_invoice: bool,
    invoice: OutboundInvoiceInput,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong phiếu xuất".into());
    }
    // Loại xuất: "sale" (bán hàng, mặc định) | "adjust" (điều chỉnh hóa đơn bán).
    // Hướng điều chỉnh (khi adjust): down = khách trả lại/giảm doanh thu, up = tăng.
    let outbound_type = if outbound_type.is_empty() {
        "sale"
    } else {
        &outbound_type
    };
    if outbound_type != "sale" && outbound_type != "adjust" {
        return Err("Loại xuất không hợp lệ (sale | adjust)".into());
    }
    let adjust_dir = if outbound_type == "adjust" {
        if adjust_dir != "down" && adjust_dir != "up" {
            return Err("Hướng điều chỉnh không hợp lệ (down | up)".into());
        }
        adjust_dir.as_str()
    } else {
        ""
    };
    let unit_code = if unit_code.is_empty() {
        "HKD".to_string()
    } else {
        unit_code
    };
    // "Lập kèm hóa đơn bán hàng" (tab Hóa đơn): tắt → bỏ qua hoàn toàn (rỗng số
    // hóa đơn); bật mà chưa nhập số → mặc định số hóa đơn = số phiếu xuất (liên
    // kết 1-1 với phiếu, "không lệch sổ"). Chỉ phiếu thực tăng doanh thu (bán
    // thường / điều chỉnh tăng) mới được lập; phiếu trả lại không phát hành.
    let mut invoice = invoice;
    if !create_invoice {
        invoice.number = String::new();
    } else if invoice.number.trim().is_empty() {
        invoice.number = voucher_no.clone();
    }
    // "Thu tiền ngay": phiếu thu tự tạo ghi Nợ 111 tiền mặt → TK này phải có
    // trong danh mục (chỉ khi có khách hàng — chưa biết thu của ai thì không tạo,
    // và không tạo cho phiếu điều chỉnh hóa đơn).
    if receive_now && outbound_type == "sale" && !customer_code.trim().is_empty() {
        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM account WHERE code = '111'")
            .fetch_one(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
        if exists.0 == 0 {
            return Err(
                "Tài khoản '111' (tiền mặt) chưa có trong danh mục tài khoản — hãy thêm ở màn Tài khoản trước khi xuất kho thu tiền ngay".into(),
            );
        }
    }
    let r = {
        let pool = state.pool.read().await;
        save_outbound_core(
            &pool,
            &posting_date,
            &voucher_no,
            &description,
            &customer_code,
            &unit_code,
            &items,
            &note,
            receive_now,
            outbound_type,
            adjust_dir,
            &invoice,
        )
        .await?
    };
    audit(&state, "save", "outbound", &voucher_no).await;
    Ok(json!({
        "ok": true,
        "entries": r.entries,
        "revenue": r.revenue,
        "cogs": r.cogs,
        "profit": r.revenue - r.cogs,
        "pt_no": r.pt_no,
        "invoice_no": r.invoice_no,
        "invoice_status": r.invoice_status,
    })
    .to_string())
}

#[tauri::command]
pub(crate) async fn get_journal_entries(
    state: State<'_, AppState>,
    entry_type: String,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let rows: Vec<JournalEntryRow> = sqlx::query_as!(
        JournalEntryRow,
        "SELECT je.id, je.posting_date, je.voucher_no, je.entry_type, je.description,
                je.product_code, je.quantity, je.unit_price, je.amount,
                s.name AS supplier_name, c.name AS customer_name,
                je.industry_code, je.adjust_code, je.note
         FROM journal_entry je
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         WHERE (? = '' OR je.entry_type = ?)
           AND (? = '' OR je.posting_date >= ?)
           AND (? = '' OR je.posting_date <= ?)
         ORDER BY je.posting_date DESC, je.id DESC
         LIMIT 500",
        entry_type,
        entry_type,
        from_date,
        from_date,
        to_date,
        to_date
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Chi tiết 1 chứng từ theo số phiếu — phục vụ in PNK (mẫu 01-VT) / PXK (mẫu 02-VT).
/// Gộp tên sản phẩm, nhà cung cấp / khách hàng để trình bày phiếu in.
#[tauri::command]
pub(crate) async fn get_voucher(
    state: State<'_, AppState>,
    voucher_no: String,
) -> Result<String, String> {
    if voucher_no.is_empty() {
        return Err("Thiếu số phiếu".into());
    }
    let rows: Vec<VoucherRow> = sqlx::query_as::<_, VoucherRow>(
        "SELECT je.posting_date, je.voucher_no, je.doc_date, je.entry_type, je.description,
                je.product_code, COALESCE(p.name, '') AS product_name, COALESCE(p.unit, '') AS unit,
                je.supplier_code, COALESCE(s.name, '') AS supplier_name,
                je.customer_code, COALESCE(c.name, '') AS customer_name,
                je.quantity, je.unit_price, je.amount, je.debit_account, je.credit_account,
                je.industry_code, je.note
         FROM journal_entry je
         LEFT JOIN product p ON p.code = je.product_code
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         WHERE je.voucher_no = ?
         ORDER BY je.id ASC",
    )
    .bind(&voucher_no)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err(format!("Không tìm thấy chứng từ '{}'", voucher_no));
    }
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_stock_lots(
    state: State<'_, AppState>,
    product_code: String,
) -> Result<String, String> {
    let rows: Vec<StockLotRow> = sqlx::query_as!(
        StockLotRow,
        "SELECT sl.id, p.code AS product_code, p.name AS product_name,
                w.code AS warehouse_code, sl.quantity, sl.unit_cost,
                sl.received_at, sl.depleted
         FROM stock_lot sl
         JOIN product p ON p.id = sl.product_id
         JOIN warehouse w ON w.id = sl.warehouse_id
         WHERE (? = '' OR p.code = ?) AND sl.depleted = 0
         ORDER BY p.code, sl.received_at ASC, sl.id ASC",
        product_code,
        product_code
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn out(product_code: &str, quantity: f64, unit_price: f64) -> OutboundItemInput {
        OutboundItemInput {
            product_code: product_code.into(),
            quantity,
            unit_price,
            industry_code: "PPHH".into(),
            warehouse_code: "".into(), // rỗng → kho mặc định / kho đầu tiên
        }
    }

    fn out_wh(
        product_code: &str,
        quantity: f64,
        unit_price: f64,
        warehouse_code: &str,
    ) -> OutboundItemInput {
        OutboundItemInput {
            product_code: product_code.into(),
            quantity,
            unit_price,
            industry_code: "PPHH".into(),
            warehouse_code: warehouse_code.into(),
        }
    }

    fn inp(product_code: &str, quantity: f64, unit_price: f64) -> InboundItemInput {
        InboundItemInput {
            product_code: product_code.into(),
            quantity,
            unit_price,
            discount: 0.0,
        }
    }

    // ─── allocate_fifo (thuần) ───

    #[test]
    fn fifo_mot_lo() {
        let lots = [(1, 10.0, 1000.0)];
        let (cogs, takes) = allocate_fifo(&lots, 4.0).unwrap();
        assert_eq!(cogs, 4000.0);
        assert_eq!(
            takes,
            vec![FifoTake {
                lot_id: 1,
                new_qty: 6.0,
                depleted: false
            }]
        );
    }

    #[test]
    fn fifo_nhieu_lo() {
        let lots = [(1, 10.0, 1000.0), (2, 10.0, 1200.0)];
        let (cogs, takes) = allocate_fifo(&lots, 15.0).unwrap();
        assert_eq!(cogs, 16_000.0); // 10x1000 + 5x1200
        assert_eq!(
            takes,
            vec![
                FifoTake {
                    lot_id: 1,
                    new_qty: 0.0,
                    depleted: true
                },
                FifoTake {
                    lot_id: 2,
                    new_qty: 5.0,
                    depleted: false
                },
            ]
        );
    }

    #[test]
    fn fifo_khop_bien() {
        let lots = [(1, 3.0, 100.0), (2, 7.0, 200.0)];
        let (cogs, takes) = allocate_fifo(&lots, 10.0).unwrap();
        assert_eq!(cogs, 1700.0); // 3x100 + 7x200
        assert!(takes.iter().all(|t| t.depleted));
        assert!(takes.iter().all(|t| t.new_qty == 0.0));
    }

    #[test]
    fn fifo_thieu_hang_tra_so_thieu() {
        let lots = [(1, 2.0, 100.0)];
        assert_eq!(allocate_fifo(&lots, 5.0), Err(3.0));
        // Không có lô nào
        assert_eq!(allocate_fifo(&[], 4.0), Err(4.0));
    }

    // ─── save_outbound_core (DB) ───

    #[tokio::test]
    async fn xuat_kho_fifo_va_ghi_so_ban_hang() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        // Kho mặc định được migration seed sẵn (KHO-CHINH) — khi dòng bỏ trống kho,
        // xuất rơi vào kho đầu tiên này (fallback của resolve_warehouse).
        let (w,): (i64,) = sqlx::query_as("SELECT id FROM warehouse WHERE code = 'KHO-CHINH'")
            .fetch_one(&pool)
            .await
            .unwrap();
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w, 10.0, 1200.0, "2026-02-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-01",
            "Bán hàng",
            "",
            "HKD",
            &[out("P1", 15.0, 2000.0)],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho");

        assert_eq!(r.entries, 1);
        assert_eq!(r.revenue, 30_000.0); // 15 x 2000
        assert_eq!(r.cogs, 16_000.0); // FIFO: 10x1000 + 5x1200

        // Lô 1 cạn, lô 2 còn 5
        let (qty1, dep1): (f64, bool) = sqlx::query_as(
            "SELECT quantity, depleted FROM stock_lot ORDER BY received_at ASC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let (qty2, dep2): (f64, bool) = sqlx::query_as(
            "SELECT quantity, depleted FROM stock_lot ORDER BY received_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((qty1, dep1), (0.0, true));
        assert_eq!((qty2, dep2), (5.0, false));

        // Sổ bán hàng: Nợ 131 / Có 511, đúng tỷ lệ ngành PPHH
        let (etype, debit, credit, amount, vat): (String, String, String, f64, f64) =
            sqlx::query_as(
                "SELECT entry_type, debit_account, credit_account, amount, vat_rate
             FROM journal_entry WHERE voucher_no = 'PXK-01'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(etype, "PX");
        assert_eq!(debit, "131");
        assert_eq!(credit, "511");
        assert_eq!(amount, 30_000.0);
        assert_eq!(vat, 0.01);
    }

    #[tokio::test]
    async fn xuat_kho_thieu_ton_bao_loi_va_khong_doi_du_lieu() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;

        let err = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-02",
            "Bán hàng",
            "",
            "HKD",
            &[out("P1", 8.0, 2000.0)],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"));

        // Không có bút toán nào được ghi và lô giữ nguyên (rollback)
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry").await,
            0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT quantity FROM stock_lot").await,
            5.0
        );
    }

    #[tokio::test]
    async fn xuat_kho_theo_dung_kho_da_chon_tren_dong() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        let w2 = seed_warehouse(&pool, "W2", "Kho 2").await;
        add_stock_lot(&pool, p, w1, 10.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w2, 3.0, 1000.0, "2026-01-01").await;

        // Xuất 3 từ W2 → vừa đủ tồn W2, W1 giữ nguyên 10.
        let r = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-W2",
            "Bán hàng kho 2",
            "",
            "HKD",
            &[out_wh("P1", 3.0, 2000.0, "W2")],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất đúng kho W2");
        assert_eq!(r.cogs, 3_000.0); // 3 x 1000 (lô W2)
        let (q1, q_w1): (f64, i64) =
            sqlx::query_as("SELECT quantity, warehouse_id FROM stock_lot WHERE warehouse_id = ?")
                .bind(w1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(q1, 10.0);
        assert_eq!(q_w1, w1);

        // Xuất tiếp 1 từ W2 → thiếu (W2 đã hết, W1 còn nhưng không bị đụng tới).
        let err = save_outbound_core(
            &pool,
            "2026-03-11",
            "PXK-W2B",
            "Bán hàng kho 2",
            "",
            "HKD",
            &[out_wh("P1", 1.0, 2000.0, "W2")],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"));
        assert!(err.contains("W2"));
    }

    #[tokio::test]
    async fn xuat_kho_dich_vu_khong_can_ton_kho() {
        let pool = test_pool().await;
        // Dịch vụ (nhân công): không có lô tồn kho nào; nhóm ngành gán trên sản phẩm
        // (DVXD-KNL → GTGT 5%, TNCN 2%) để kiểm tra fallback khi dòng bỏ trống nhóm.
        seed_product(&pool, "S1", "Nhân công", 0.0).await;
        sqlx::query(
            "UPDATE product SET is_service = 1, industry_code = 'DVXD-KNL' WHERE code = 'S1'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let r = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-SV",
            "Bán dịch vụ",
            "",
            "HKD",
            &[OutboundItemInput {
                product_code: "S1".into(),
                quantity: 5.0,
                unit_price: 300_000.0,
                industry_code: "".into(), // rỗng → lấy nhóm ngành của sản phẩm
                warehouse_code: "".into(),
            }],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất dịch vụ không cần tồn kho");

        // Chỉ ghi doanh thu, không giá vốn, không đụng stock_lot.
        assert_eq!(r.entries, 1);
        assert_eq!(r.revenue, 1_500_000.0);
        assert_eq!(r.cogs, 0.0);
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM stock_lot").await, 0);

        let (etype, debit, credit, amount, industry, vat, pit): (
            String,
            String,
            String,
            f64,
            String,
            f64,
            f64,
        ) = sqlx::query_as(
            "SELECT entry_type, debit_account, credit_account, amount, industry_code, vat_rate, pit_rate
             FROM journal_entry WHERE voucher_no = 'PXK-SV'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(etype, "PX");
        assert_eq!(debit, "131");
        assert_eq!(credit, "511");
        assert_eq!(amount, 1_500_000.0);
        assert_eq!(industry, "DVXD-KNL");
        assert_eq!(vat, 0.05);
        assert_eq!(pit, 0.02);
    }

    // ─── Thu tiền ngay: tự tạo phiếu thu (PT) liên kết thanh toán của khách hàng ───

    #[tokio::test]
    async fn xuat_kho_thu_tien_ngay_tu_tao_phieu_thu() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        // Bán 5 x 2.000 thu tiền ngay → PT Nợ 111 / Có 131: 10.000
        let r = save_outbound_core(
            &pool,
            "2026-05-15",
            "PXK-03",
            "Bán hàng thu tiền ngay",
            "KH1",
            "HKD",
            &[out_wh("P1", 5.0, 2000.0, "W1")],
            "",
            true,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho thu tiền ngay");

        assert_eq!(r.revenue, 10_000.0);
        assert_eq!(r.cogs, 5_000.0);
        assert_eq!(r.pt_no, "PT001");

        // Phiếu thu: Nợ 111 tiền mặt / Có 131 phải thu, đủ doanh thu
        let (etype, debit, credit, amount, cus): (String, String, String, f64, String) =
            sqlx::query_as(
                "SELECT entry_type, debit_account, credit_account, amount, customer_code
                 FROM journal_entry WHERE voucher_no = 'PT001'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(etype, "PT");
        assert_eq!(debit, "111");
        assert_eq!(credit, "131");
        assert_eq!(amount, 10_000.0);
        assert_eq!(cus, "KH1");
        // Liên kết: ghi chú chứa số PX để truy vết PT ↔ PX
        let (note,): (String,) =
            sqlx::query_as("SELECT note FROM journal_entry WHERE voucher_no = 'PT001'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(note.contains("PXK-03"));

        // Số PT tiếp theo tự tăng: PT002
        let r2 = save_outbound_core(
            &pool,
            "2026-05-16",
            "PXK-04",
            "Bán hàng 2",
            "KH1",
            "HKD",
            &[out_wh("P1", 1.0, 1000.0, "W1")],
            "",
            true,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho 2");
        assert_eq!(r2.pt_no, "PT002");
    }

    // Thu tiền ngay nhưng chưa chọn khách hàng → không tạo phiếu thu.
    #[tokio::test]
    async fn xuat_kho_thu_tien_ngay_khong_kh_thi_khong_tao_pt() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-05-17",
            "PXK-05",
            "Bán không ghi khách",
            "",
            "HKD",
            &[out_wh("P1", 1.0, 1000.0, "W1")],
            "",
            true,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho");
        assert_eq!(r.pt_no, "");
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PT'"
            )
            .await,
            0
        );
    }

    // ─── save_inbound_core (DB) ───

    #[tokio::test]
    async fn nhap_kho_tao_lo_va_ghi_so_nhap() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        let r = save_inbound_core(
            &pool,
            "2026-01-05",
            "PNK-01",
            "Nhập hàng",
            "",
            "W1",
            "HKD",
            &[inp("P1", 5.0, 1000.0), inp("P1", 3.0, 1200.0)],
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
        )
        .await
        .expect("nhập kho");

        assert_eq!(r.entries, 2);
        assert_eq!(r.total, 8600.0); // 5x1000 + 3x1200
        assert_eq!(r.vat, 0.0);

        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM stock_lot").await, 2);
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PN' AND voucher_no = 'PNK-01'"
            )
            .await,
            2
        );
        // Tồn kho tổng = 8
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            8.0
        );
    }

    // ─── Nhập kho khấu trừ GTGT đầu vào (app_setting vat_deduct = '1') ───

    #[tokio::test]
    async fn nhap_kho_khau_tru_vat_tach_bu_toan_133() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        // Mua 10 x 10.000, VAT 10% → Nợ 152/Có 331: 100.000 + Nợ 133/Có 331: 10.000
        let r = save_inbound_core(
            &pool,
            "2026-02-10",
            "PNK-02",
            "Mua hàng theo HĐ 123",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 10.0, 10_000.0)],
            "",
            "purchase",
            "HĐ 123 ngày 10/02/2026",
            0.1,
            "152",
            "331",
            false,
            "",
        )
        .await
        .expect("nhập kho khấu trừ");

        assert_eq!(r.total, 100_000.0);
        assert_eq!(r.vat, 10_000.0);

        // 2 bút toán: 1 ghi hàng (Nợ 152) + 1 ghi thuế (Nợ 133); dòng thuế SL = 0.
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PN' AND voucher_no = 'PNK-02'").await,
            2
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT COALESCE(SUM(amount),0) FROM journal_entry WHERE voucher_no = 'PNK-02' AND debit_account = '133'").await,
            10_000.0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT COALESCE(SUM(amount),0) FROM journal_entry WHERE voucher_no = 'PNK-02' AND debit_account = '152'").await,
            100_000.0
        );
        // Dòng thuế không ảnh hưởng số lượng tồn kho
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            10.0
        );
        // Ghi chú kèm số chứng từ gốc (cột "Theo..." mẫu 03-VT)
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = 'PNK-02' AND note LIKE 'Theo chứng từ: HĐ 123%'").await,
            2
        );
    }

    // ─── Nhập kho tự sản xuất / gia công (Nợ 155 / Có 154, không cần NCC) ───

    #[tokio::test]
    async fn nhap_kho_tu_san_xuat_hach_toan_155_154() {
        let pool = test_pool().await;
        seed_product(&pool, "TP1", "Thành phẩm A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        // Nhập thành phẩm theo lệnh nhập kho — giá thành 8.000/sp
        let r = save_inbound_core(
            &pool,
            "2026-03-15",
            "PNK-03",
            "Nhập kho thành phẩm đợt 1",
            "",
            "W1",
            "HKD",
            &[inp("TP1", 50.0, 8_000.0)],
            "",
            "production",
            "Lệnh SX 015",
            0.0,
            "155",
            "154",
            false,
            "",
        )
        .await
        .expect("nhập kho sản xuất");

        assert_eq!(r.total, 400_000.0);
        assert_eq!(r.vat, 0.0);

        // 1 bút toán duy nhất: Nợ 155 / Có 154
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PN' AND voucher_no = 'PNK-03'").await,
            1
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT COALESCE(SUM(amount),0) FROM journal_entry WHERE voucher_no = 'PNK-03' AND debit_account = '155' AND credit_account = '154'").await,
            400_000.0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            50.0
        );
    }

    // ─── Nhập kho chiết khấu thương mại (CT MH: Thành tiền − Tiền CK) ───

    #[tokio::test]
    async fn nhap_kho_chiec_khau_tru_vao_gia_tri_nhap() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        // Mua 10 x 10.000, tiền CK 5.000đ → giá trị nhập kho = 95.000, giá vốn 9.500
        let r = save_inbound_core(
            &pool,
            "2026-04-05",
            "PNK-04",
            "Mua hàng có chiết khấu",
            "NCC1",
            "W1",
            "HKD",
            &[InboundItemInput {
                product_code: "P1".into(),
                quantity: 10.0,
                unit_price: 10_000.0,
                discount: 5_000.0,
            }],
            "",
            "purchase",
            "HĐ 88",
            0.0,
            "152",
            "331",
            false,
            "",
        )
        .await
        .expect("nhập kho chiết khấu");

        assert_eq!(r.total, 95_000.0); // 10 × 10.000 − 5.000
        assert_eq!(r.vat, 0.0);

        // Giá vốn FIFO = đơn giá sau chiết khấu; bút toán ghi đúng giá trị nhập kho
        assert_eq!(
            scalar_f64(&pool, "SELECT unit_cost FROM stock_lot").await,
            9_500.0
        );
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT amount FROM journal_entry WHERE voucher_no = 'PNK-04' AND debit_account = '152'"
            )
            .await,
            95_000.0
        );
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PN' AND voucher_no = 'PNK-04'").await,
            1
        );
    }

    // ─── Trả tiền ngay: tự tạo phiếu chi (PC) liên kết thanh toán cho NCC ───

    #[tokio::test]
    async fn nhap_kho_tra_tien_ngay_tu_tao_phieu_chi() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        // Mua 10 x 10.000, VAT 10% khấu trừ, trả tiền ngay → PC Nợ 331 / Có 111: 110.000
        let r = save_inbound_core(
            &pool,
            "2026-05-10",
            "PNK-05",
            "Mua hàng trả tiền ngay",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 10.0, 10_000.0)],
            "",
            "purchase",
            "HĐ 200",
            0.1,
            "152",
            "331",
            true,
            "",
        )
        .await
        .expect("nhập kho trả tiền ngay");

        assert_eq!(r.total, 100_000.0);
        assert_eq!(r.vat, 10_000.0);

        // Phiếu chi: Nợ 331 (phải trả NCC) / Có 111 (tiền mặt), đủ 110.000 = hàng + thuế
        let (etype, debit, credit, amount, vat): (String, String, String, f64, f64) =
            sqlx::query_as(
                "SELECT entry_type, debit_account, credit_account, amount, vat_rate
                 FROM journal_entry WHERE voucher_no = 'PC001'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(etype, "PC");
        assert_eq!(debit, "331");
        assert_eq!(credit, "111");
        assert_eq!(amount, 110_000.0);
        assert_eq!(vat, 0.0);
        // Liên kết: ghi chú chứa số PNK + gắn đúng nhà cung cấp
        let (sup, note): (String, String) = sqlx::query_as(
            "SELECT supplier_code, note FROM journal_entry WHERE voucher_no = 'PC001'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sup, "NCC1");
        assert!(note.contains("PNK-05"));

        // Số PC tiếp theo tự tăng: PC002
        let r2 = save_inbound_core(
            &pool,
            "2026-05-11",
            "PNK-06",
            "Mua hàng 2",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 1.0, 1_000.0)],
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            true,
            "",
        )
        .await
        .expect("nhập kho 2");
        assert_eq!(r2.pc_no, "PC002");
    }

    // Trả tiền ngay nhưng chưa chọn nhà cung cấp → không tạo phiếu chi.
    #[tokio::test]
    async fn nhap_kho_tra_tien_ngay_khong_ncc_thi_khong_tao_pc() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        let r = save_inbound_core(
            &pool,
            "2026-05-12",
            "PNK-07",
            "Nhập không NCC",
            "",
            "W1",
            "HKD",
            &[inp("P1", 2.0, 500.0)],
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            true,
            "",
        )
        .await
        .expect("nhập kho");
        assert_eq!(r.pc_no, "");
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PC'"
            )
            .await,
            0
        );
    }

    // Trả tiền ngay nhưng PNK đã trả thẳng tiền mặt (Có 111) → không tạo PC kép.
    #[tokio::test]
    async fn nhap_kho_credit_tien_mat_khong_tao_pc_doi() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        let r = save_inbound_core(
            &pool,
            "2026-05-13",
            "PNK-08",
            "Mua trả tiền mặt",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 2.0, 500.0)],
            "",
            "purchase",
            "",
            0.0,
            "152",
            "111",
            true,
            "",
        )
        .await
        .expect("nhập kho");
        assert_eq!(r.pc_no, "");
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PC'"
            )
            .await,
            0
        );
    }

    // ─── Điều chỉnh hóa đơn mua (inbound_type = "adjust") ───

    // Giảm (down): bên bán trừ bớt → trả lại NCC. Không tạo lô mới — trừ FIFO khỏi
    // lô theo ngày nhập + ghi sổ đảo ngược: Nợ TK đối ứng (331) / Có TK hàng (152),
    // kèm giảm thuế GTGT đầu vào (Nợ 331 / Có 133) khi khấu trừ.
    #[tokio::test]
    async fn nhap_kho_dieu_chinh_giam_tra_lai_ncc() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w, 10.0, 1200.0, "2026-02-01").await;

        // Trả lại NCC 5 (đơn giá 1.000) → trừ FIFO lô đầu (5/10), ghi đảo ngược, VAT 10%.
        let r = save_inbound_core(
            &pool,
            "2026-06-01",
            "PNK-DC01",
            "Điều chỉnh hóa đơn mua (trả lại NCC)",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 5.0, 1_000.0)],
            "",
            "adjust",
            "HĐĐC 01",
            0.1,
            "152",
            "331",
            false,
            "down",
        )
        .await
        .expect("điều chỉnh giảm hóa đơn mua");

        assert_eq!(r.entries, 1); // số dòng mặt hàng
        assert_eq!(r.total, 5_000.0);
        assert_eq!(r.vat, 500.0); // 5.000 × 10%

        // Tồn giảm xuống 15 — KHÔNG tạo lô mới
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            15.0
        );
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM stock_lot").await, 2);
        let (q1,): (f64,) =
            sqlx::query_as("SELECT quantity FROM stock_lot ORDER BY received_at ASC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(q1, 5.0); // lô đầu trừ 5

        // Sổ đảo ngược: Nợ 331 / Có 152 (hàng) + Nợ 331 / Có 133 (thuế), adjust = GiamCP
        let (debit, credit, adj): (String, String, String) = sqlx::query_as(
            "SELECT debit_account, credit_account, adjust_code FROM journal_entry
             WHERE voucher_no = 'PNK-DC01' AND debit_account = '331' AND credit_account = '152'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (debit.as_str(), credit.as_str(), adj.as_str()),
            ("331", "152", "GiamCP")
        );
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT COALESCE(SUM(amount),0) FROM journal_entry
                 WHERE voucher_no = 'PNK-DC01' AND credit_account = '133'"
            )
            .await,
            500.0
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = 'PNK-DC01' AND adjust_code = 'GiamCP'"
            )
            .await,
            2
        );
    }

    // Giảm nhưng không đủ tồn → báo lỗi và rollback toàn bộ.
    #[tokio::test]
    async fn nhap_kho_dieu_chinh_giam_thieu_ton_bao_loi() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 3.0, 1000.0, "2026-01-01").await;

        let err = save_inbound_core(
            &pool,
            "2026-06-02",
            "PNK-DC02",
            "Trả lại NCC",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 5.0, 1_000.0)],
            "",
            "adjust",
            "",
            0.0,
            "152",
            "331",
            false,
            "down",
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"));

        // Rollback: không ghi bút toán, lô giữ nguyên
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry").await,
            0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT quantity FROM stock_lot").await,
            3.0
        );
    }

    // Tăng (up): bên bán thêm → nhập kho như phiếu thường (Nợ 152 / Có 331).
    #[tokio::test]
    async fn nhap_kho_dieu_chinh_tang_nhap_nhu_thuong() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        let r = save_inbound_core(
            &pool,
            "2026-06-03",
            "PNK-DC03",
            "Điều chỉnh hóa đơn mua (tăng)",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 4.0, 2_000.0)],
            "",
            "adjust",
            "",
            0.0,
            "152",
            "331",
            false,
            "up",
        )
        .await
        .expect("điều chỉnh tăng hóa đơn mua");

        assert_eq!(r.entries, 1);
        assert_eq!(r.total, 8_000.0);
        assert_eq!(r.vat, 0.0);
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            4.0
        );
        let (debit, credit): (String, String) = sqlx::query_as(
            "SELECT debit_account, credit_account FROM journal_entry WHERE voucher_no = 'PNK-DC03'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((debit.as_str(), credit.as_str()), ("152", "331"));
    }

    // ─── Điều chỉnh hóa đơn bán (outbound_type = "adjust") ───

    // Giảm (down): khách trả lại → nhập lại hàng về kho (lô mới theo đơn giá điều
    // chỉnh), giảm doanh thu (Nợ 511 / Có 131, adjust = GiamDT) + giảm giá vốn
    // (Nợ 152 / Có 632). Không xuất FIFO, không tạo phiếu thu.
    #[tokio::test]
    async fn xuat_kho_dieu_chinh_giam_khach_tra_lai() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-06-10",
            "PXK-DC01",
            "Điều chỉnh hóa đơn bán (khách trả lại)",
            "KH1",
            "HKD",
            &[out_wh("P1", 3.0, 2000.0, "W1")],
            "",
            false,
            "adjust",
            "down",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("điều chỉnh giảm hóa đơn bán");

        assert_eq!(r.entries, 1); // số dòng mặt hàng
        assert_eq!(r.revenue, 6_000.0);
        assert_eq!(r.cogs, 0.0); // không xuất FIFO
        assert_eq!(r.pt_no, "");

        // Tồn tăng +3 (lô mới, đơn giá 2.000 = đơn giá điều chỉnh)
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            23.0
        );
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM stock_lot").await, 2);
        let (cost,): (f64,) =
            sqlx::query_as("SELECT unit_cost FROM stock_lot ORDER BY id DESC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cost, 2000.0);

        // PX: Nợ 511 / Có 131 (giảm doanh thu), adjust = GiamDT
        let (debit, credit, adj): (String, String, String) = sqlx::query_as(
            "SELECT debit_account, credit_account, adjust_code FROM journal_entry
             WHERE voucher_no = 'PXK-DC01' AND entry_type = 'PX'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (debit.as_str(), credit.as_str(), adj.as_str()),
            ("511", "131", "GiamDT")
        );
        // PN: Nợ 152 / Có 632 (nhập lại hàng, giảm giá vốn)
        let (debit, credit): (String, String) = sqlx::query_as(
            "SELECT debit_account, credit_account FROM journal_entry
             WHERE voucher_no = 'PXK-DC01' AND entry_type = 'PN'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((debit.as_str(), credit.as_str()), ("152", "632"));
    }

    // Điều chỉnh giảm — kể cả "thu tiền ngay = true" cũng KHÔNG tạo phiếu thu.
    #[tokio::test]
    async fn xuat_kho_dieu_chinh_giam_khong_tao_phieu_thu() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-06-11",
            "PXK-DC02",
            "Điều chỉnh hóa đơn bán",
            "KH1",
            "HKD",
            &[out_wh("P1", 1.0, 2000.0, "W1")],
            "",
            true,
            "adjust",
            "down",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("điều chỉnh");
        assert_eq!(r.pt_no, "");
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PT'"
            )
            .await,
            0
        );
    }

    // Tăng (up): ghi doanh thu như phiếu bán thường (FIFO), nhưng không tạo PT.
    #[tokio::test]
    async fn xuat_kho_dieu_chinh_tang_ghi_doanh_thu_khong_tao_pt() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-06-12",
            "PXK-DC03",
            "Điều chỉnh hóa đơn bán (tăng)",
            "KH1",
            "HKD",
            &[out_wh("P1", 2.0, 3000.0, "W1")],
            "",
            true,
            "adjust",
            "up",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("điều chỉnh tăng hóa đơn bán");

        assert_eq!(r.revenue, 6_000.0);
        assert_eq!(r.cogs, 2_000.0); // xuất FIFO bình thường
        assert_eq!(r.pt_no, "");
        assert_eq!(
            scalar_f64(&pool, "SELECT SUM(quantity) FROM stock_lot").await,
            18.0
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PT'"
            )
            .await,
            0
        );
    }

    // ─── Phiếu xuất tự lập hóa đơn bán hàng kèm (tab "Hóa đơn") ───

    // Bán hàng bật "Lập kèm hóa đơn": hóa đơn số = số phiếu xuất, status 'draft'
    // (chưa khai HĐĐT), liên kết voucher_no = PX, tổng tiền = doanh thu; tên +
    // MST khách hàng lấy từ danh mục; đủ dòng chi tiết với nhóm ngành + tỷ lệ thuế.
    #[tokio::test]
    async fn xuat_kho_tu_dong_lap_hoa_don_nhap() {
        let pool = test_pool().await;
        seed_customer(&pool, "KH1", "Cửa hàng Mây").await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;

        let inv = OutboundInvoiceInput {
            number: "PXK-INV1".into(),
            ..Default::default()
        };
        let r = save_outbound_core(
            &pool,
            "2026-07-01",
            "PXK-INV1",
            "Bán hàng lập hóa đơn",
            "KH1",
            "HKD",
            &[out_wh("P1", 3.0, 2000.0, "W1")],
            "",
            false,
            "sale",
            "",
            &inv,
        )
        .await
        .expect("xuất kho kèm hóa đơn");

        assert_eq!(r.invoice_no, "PXK-INV1");
        assert_eq!(r.invoice_status, "draft");

        let (number, status, customer, tax, total, vno): (
            String,
            String,
            String,
            String,
            f64,
            String,
        ) = sqlx::query_as(
            "SELECT number, status, customer, customer_tax_code, total, voucher_no
             FROM invoice",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((number.as_str(), status.as_str()), ("PXK-INV1", "draft"));
        assert_eq!(customer, "Cửa hàng Mây");
        assert_eq!(tax, "MST-KH1");
        assert_eq!(total, 6_000.0); // 3 x 2000
        assert_eq!(vno, "PXK-INV1");

        // Dòng chi tiết hóa đơn: đủ hàng, nhóm ngành + tỷ lệ thuế theo bút toán.
        let (qty, price, subtotal, ind): (f64, f64, f64, String) = sqlx::query_as(
            "SELECT quantity, unit_price, subtotal, industry_code FROM invoice_item",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((qty, price, subtotal), (3.0, 2000.0, 6_000.0));
        assert_eq!(ind, "PPHH");
    }

    // Khai Số HĐĐT + Ký hiệu + Ngày ngay trên phiếu → hóa đơn kèm chuyển thành
    // "Đã liên kết HĐĐT" (official) trong chính lần lưu phiếu.
    #[tokio::test]
    async fn xuat_kho_kem_hddt_thanh_hoa_don_lien_ket() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;

        let inv = OutboundInvoiceInput {
            number: "PXK-HD1".into(),
            e_invoice_no: "E2026/001".into(),
            e_invoice_symbol: "1C26TT152".into(),
            e_invoice_date: "2026-07-02".into(),
        };
        let r = save_outbound_core(
            &pool,
            "2026-07-02",
            "PXK-HD1",
            "Bán hàng phát hành HĐĐT",
            "",
            "HKD",
            &[out_wh("P1", 2.0, 5000.0, "W1")],
            "",
            false,
            "sale",
            "",
            &inv,
        )
        .await
        .expect("xuất kho kèm HĐĐT");

        assert_eq!(r.invoice_status, "official");
        let (status, eno, esym, edate): (String, String, String, String) = sqlx::query_as(
            "SELECT status, e_invoice_no, e_invoice_symbol, e_invoice_date FROM invoice",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "official");
        assert_eq!(eno, "E2026/001");
        assert_eq!(esym, "1C26TT152");
        assert_eq!(edate, "2026-07-02");
    }

    // Điều chỉnh GIẢM (khách trả lại) — dù gửi kèm thông tin hóa đơn cũng KHÔNG
    // lập hóa đơn mới (chỉ ghi đảo doanh thu / nhập lại hàng về kho).
    #[tokio::test]
    async fn xuat_kho_dieu_chinh_giam_khong_lap_hoa_don() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let inv = OutboundInvoiceInput {
            number: "PXK-DC-RET".into(),
            ..Default::default()
        };
        let r = save_outbound_core(
            &pool,
            "2026-07-03",
            "PXK-DC-RET",
            "Khách trả lại",
            "",
            "HKD",
            &[out("P1", 2.0, 2000.0)],
            "",
            false,
            "adjust",
            "down",
            &inv,
        )
        .await
        .expect("điều chỉnh giảm");
        assert_eq!(r.invoice_no, "");
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM invoice").await, 0);
    }

    // Không bật "Lập kèm hóa đơn" (số hóa đơn rỗng) → không tạo bản ghi hóa đơn.
    #[tokio::test]
    async fn xuat_kho_khong_lap_hoa_don_khi_tat() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-07-04",
            "PXK-NOINV",
            "Bán lẻ không cần hóa đơn",
            "",
            "HKD",
            &[out_wh("P1", 1.0, 2000.0, "W1")],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho không kèm hóa đơn");
        assert_eq!(r.invoice_no, "");
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM invoice").await, 0);
    }

    // Điều chỉnh TĂNG cũng là phiếu tăng doanh thu → lập hóa đơn kèm như bán thường.
    #[tokio::test]
    async fn xuat_kho_dieu_chinh_tang_tu_lap_hoa_don() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        let inv = OutboundInvoiceInput {
            number: "PXK-DC-UP".into(),
            ..Default::default()
        };
        let r = save_outbound_core(
            &pool,
            "2026-07-05",
            "PXK-DC-UP",
            "Điều chỉnh tăng",
            "KH1",
            "HKD",
            &[out_wh("P1", 2.0, 3000.0, "W1")],
            "",
            false,
            "adjust",
            "up",
            &inv,
        )
        .await
        .expect("điều chỉnh tăng kèm hóa đơn");
        assert_eq!(r.invoice_no, "PXK-DC-UP");
        assert_eq!(r.invoice_status, "draft");
        assert_eq!(
            scalar_f64(&pool, "SELECT total FROM invoice").await,
            6_000.0 // 2 x 3000
        );
    }
}
