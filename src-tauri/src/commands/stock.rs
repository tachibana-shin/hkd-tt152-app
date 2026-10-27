#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{Sqlite, SqlitePoolOptions, SqliteTransaction};
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
    /// id đầu phiếu `inbound_voucher` — HĐĐT dùng để neo liên kết 1-1.
    pub(crate) inbound_voucher_id: i64,
    pub(crate) entries: usize,
    /// Tổng giá trị hàng nhập kho: chưa thuế nếu hộ khấu trừ GTGT, gồm thuế nếu không.
    pub(crate) total: f64,
    /// Tổng thuế GTGT đầu vào được khấu trừ (0 nếu không khấu trừ).
    pub(crate) vat: f64,
    /// Số phiếu chi (PC) tự tạo khi bật "Trả tiền ngay" (rỗng nếu không tạo).
    pub(crate) pc_no: String,
    /// Số phiếu xuất NVL tự sinh theo định mức (rỗng nếu không tự xuất).
    pub(crate) material_voucher: String,
}

/// Thông tin đầu phiếu ngoài các tham số cũ: nguồn tạo phiếu + hóa đơn gốc.
///
/// `overwrite = true` bỏ chốt "số phiếu đã tồn tại" để nhập lại đè lên phiếu
/// nhập tay đã khớp hóa đơn (caller đã xoá bút toán/lô cũ trong CÙNG transaction).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InboundExtra<'a> {
    /// `manual` (UI) | `hddt` (đồng bộ cổng) | `legacy` (dữ liệu cũ).
    pub(crate) source: &'a str,
    pub(crate) invoice_serial: &'a str,
    pub(crate) invoice_no: &'a str,
    pub(crate) invoice_date: &'a str,
    pub(crate) overwrite: bool,
    /// Bỏ qua chính phiếu này khi kiểm trùng ký hiệu/số/ngày hóa đơn.
    pub(crate) exclude_id: Option<i64>,
}

// ─── F4 — TỰ XUẤT NVL THEO ĐỊNH MỨC KHI LƯU PNK SẢN XUẤT ───
//
// Đơn vị khai "1 thành phẩm cần bao nhiêu NVL" (product_bom_item). Khi lưu phiếu
// nhập loại `production`, app tự:
//   1. tính NVL cần = định mức × SL thành phẩm (cộng dồn nếu lặp dòng);
//   2. xuất FIFO NVL khỏi kho của phiếu nhập (rỗng → kho mặc định của NVL);
//   3. ghi 1 phiếu xuất (entry_type = 'PX') Nợ 154 / Có 152 ngay trong CÙNG
//      transaction với phiếu nhập — thiếu tồn thì CẢ HAI không lưu gì;
//   4. chụp snapshot định mức + đơn giá khai vào `production_lot_component`
//      (kể cả dịch vụ) để sửa định mức sau này không làm đổi lô đã tạo.
//
// Giá trị nhập thành phẩm lấy theo đơn giá KHAI trên định mức (Σ định mức ×
// đơn giá), không lấy giá vốn FIFO của NVL — FIFO chỉ dùng để ghi Có 152.
//
// Vì sao Nợ 154 (chi phí SXKD dở dang) / Có 152 (hàng hóa): xuất NVL cho sản
// xuất là chuyển giá trị hàng hóa vào chi phí dở dang, KHÔNG phải doanh thu và
// KHÔNG phải giá vốn bán hàng. Phiếu nhập thành phẩm sau đó ghi Nợ 155 / Có 154
// (mặc định) → dòng tiền 152 → 154 → 155 khép kín.
//
// `adjust_code = 'XuatNVL'` là dấu hiệu để báo cáo LOẠI dòng này khỏi:
//   • doanh thu (`get_revenue_expense_core`, sổ bán hàng S1a/S2a/S2b);
//   • giá vốn FIFO + kế hoạch bút toán Nợ 632 / Có 152 (nếu không loại, app sẽ
//     ghi thêm 1 bút 632/152 → credit 152 lần thứ hai → sai tồn giá trị).
// Sổ kho (`get_inventory_summary`, số đầu kỳ) và danh sách phiếu xuất vẫn TÍNH
// (entry_type = 'PX') — đúng: tồn NVL thực tế đã giảm.
/// Một dòng định mức đã chụp cho lô sản xuất: 1 NVL/dịch vụ của 1 dòng thành
/// phẩm, kèm số lượng thực tế của lô và đơn giá khai tại thời điểm lưu.
///
/// Snapshot tách khỏi `product_bom_item` vì bảng định mức được sửa tự do —
/// lô đã sản xuất phải giữ đúng thành phần/giá đã dùng, không đổi theo định
/// mức hiện hành.
#[derive(Debug, Clone)]
struct ProductionBomLine {
    /// Vị trí dòng thành phẩm trong `items` — tách được 2 dòng cùng mã hàng.
    item_index: usize,
    product_code: String,
    product_name: String,
    material_id: i64,
    material_code: String,
    material_name: String,
    material_unit: String,
    is_service: bool,
    per_unit_qty: f64,
    finished_qty: f64,
    quantity: f64,
    unit_price: f64,
    amount: f64,
    warehouse_code: String,
}

/// Đọc định mức hiện hành của mọi thành phẩm trong phiếu nhập sản xuất.
///
/// Dịch vụ (nhân công…) vẫn được chụp để đủ bảng thành phần và tính giá trị
/// lô theo đơn giá khai, nhưng không tham gia trừ tồn kho.
async fn load_production_bom_lines(
    tx: &mut SqliteTransaction<'_>,
    items: &[InboundItemInput],
    warehouse_code: &str,
) -> Result<Vec<ProductionBomLine>, String> {
    let mut out = Vec::new();
    for (item_index, item) in items.iter().enumerate() {
        if !item.quantity.is_finite() || item.quantity <= 0.0 {
            continue;
        }
        let product = resolve_product(tx, &item.product_code).await?;
        let rows = sqlx::query!(
            r#"SELECT m.id as "id!", m.code as "code!", m.name as "name!",
                      m.unit as "unit!", m.is_service as "is_service: bool",
                      b.quantity, b.unit_price
                 FROM product_bom_item b
                 JOIN product m ON m.id = b.material_product_id
                WHERE b.product_id = ?
                ORDER BY m.code"#,
            product.id
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        for r in rows {
            // Làm tròn 6 chữ số thập phân — nhân float hay dính nhiễu (0.1×3).
            let quantity = (r.quantity * item.quantity * 1e6).round() / 1e6;
            if quantity <= 0.0 {
                continue;
            }
            out.push(ProductionBomLine {
                item_index,
                product_code: item.product_code.clone(),
                product_name: product.name.clone(),
                material_id: r.id,
                material_code: r.code,
                material_name: r.name,
                material_unit: r.unit,
                is_service: r.is_service,
                per_unit_qty: r.quantity,
                finished_qty: item.quantity,
                quantity,
                unit_price: r.unit_price,
                amount: round2(quantity * r.unit_price),
                warehouse_code: warehouse_code.to_string(),
            });
        }
    }
    Ok(out)
}

/// Ghi snapshot thành phần của lô vào DB (xoá bản cũ nếu phiếu được ghi đè).
async fn snapshot_production_components(
    tx: &mut SqliteTransaction<'_>,
    inbound_id: i64,
    lines: &[ProductionBomLine],
) -> Result<(), String> {
    sqlx::query!(
        "DELETE FROM production_lot_component WHERE inbound_voucher_id = ?",
        inbound_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    for l in lines {
        sqlx::query!(
            "INSERT INTO production_lot_component
                (inbound_voucher_id, product_code, product_name, material_id,
                 material_code, material_name, material_unit, is_service,
                 per_unit_qty, finished_qty, quantity, unit_price, amount, warehouse_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            inbound_id,
            l.product_code,
            l.product_name,
            l.material_id,
            l.material_code,
            l.material_name,
            l.material_unit,
            l.is_service,
            l.per_unit_qty,
            l.finished_qty,
            l.quantity,
            l.unit_price,
            l.amount,
            l.warehouse_code
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Đơn giá thành phẩm theo đơn giá KHAI trên từng dòng định mức của MỘT dòng
/// phiếu: Σ thành tiền khai ÷ sản lượng dòng. Gồm cả dịch vụ (nhân công) vì
/// đó cũng là một phần giá trị lô. Trả `None` khi chưa khai giá (tổng = 0) để
/// caller giữ đơn giá người dùng nhập (tương thích dữ liệu cũ).
fn declared_unit_price(
    lines: &[ProductionBomLine],
    item_index: usize,
    quantity: f64,
) -> Option<f64> {
    if quantity <= 0.0 {
        return None;
    }
    let total: f64 = lines
        .iter()
        .filter(|l| l.item_index == item_index)
        .map(|l| l.amount)
        .sum();
    if total <= 0.0 {
        return None;
    }
    Some(total / quantity)
}

#[allow(clippy::too_many_arguments)]
async fn issue_materials_by_bom(
    tx: &mut SqliteTransaction<'_>,
    inbound_id: i64,
    posting_date: &str,
    pn_voucher_no: &str,
    warehouse_code: &str,
    unit_code: &str,
    cost_account: &str,
    lines: &[ProductionBomLine],
) -> Result<String, String> {
    // 1) Gom NVL cần cho từng mặt hàng của phiếu.
    //    (id, mã, tên, số lượng cần) — thứ tự theo mã vật tư cho số liệu ổn định.
    let mut needs: Vec<(i64, String, String, f64)> = Vec::new();
    for line in lines.iter().filter(|l| !l.is_service) {
        match needs.iter_mut().find(|(id, ..)| *id == line.material_id) {
            Some(n) => n.3 = ((n.3 + line.quantity) * 1e6).round() / 1e6,
            None => needs.push((
                line.material_id,
                line.material_code.clone(),
                line.material_name.clone(),
                line.quantity,
            )),
        }
    }
    needs.retain(|n| n.3 > 0.0);
    needs.sort_by(|a, b| a.1.cmp(&b.1));
    if needs.is_empty() {
        return Ok(String::new());
    }

    // 2) TK dùng cho bút toán tự sinh phải có trong DMTK (152 = hàng hóa).
    if cost_account.eq("152") {
        return Err(
            "TK Chi phí của phiếu nhập bằng TK Hàng hóa (152) — không thể tự xuất NVL theo định mức"
                .into(),
        );
    }
    for code in [cost_account, "152"] {
        let exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM account WHERE code = ?", code)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!(
                "Tài khoản '{}' chưa có trong danh mục tài khoản — hãy thêm ở màn Tài khoản trước khi tự xuất NVL",
                code
            ));
        }
    }

    // 3) Số phiếu PX tiếp theo — sinh TỪ DB trong transaction nên không đụng số.
    let existing: Vec<String> =
        sqlx::query_scalar!("SELECT voucher_no FROM journal_entry WHERE entry_type = 'PX'")
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    let px_no = format!("PX{:03}", max_voucher_seq(&existing) + 1);

    let desc = format!("Xuất NVL theo định mức — {}", pn_voucher_no);
    let note = format!("Theo {}", pn_voucher_no);

    // 4) Xuất từng NVL: FIFO trong kho của phiếu nhập + 1 dòng bút toán PX.
    for (material_id, code, name, need) in &needs {
        let wh = resolve_warehouse(tx, warehouse_code, code).await?;
        let lots: Vec<(i64, f64, f64)> = sqlx::query!(
            r#"SELECT id as "id!", quantity, unit_cost FROM stock_lot
               WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
               ORDER BY received_at ASC, id ASC"#,
            material_id,
            wh
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.id, r.quantity, r.unit_cost))
        .collect();
        let (cogs, takes) = allocate_fifo(&lots, *need).map_err(|short| {
            format!(
                "Không đủ tồn kho vật tư '{}' ({}): cần {}, còn thiếu {} — bỏ ô \"Tự xuất NVL theo định mức\" hoặc nhập thêm kho",
                code,
                name,
                crate::commands::invoice_export::fmt_qty(*need),
                crate::commands::invoice_export::fmt_qty(short),
            )
        })?;
        for t in takes {
            sqlx::query!(
                "UPDATE stock_lot SET quantity = ?, depleted = ? WHERE id = ?",
                t.new_qty,
                t.depleted,
                t.lot_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        let avg_cost = cogs / need;
        insert_journal_entry(
            tx,
            posting_date,
            &px_no,
            "PX",
            &desc,
            code,
            "",
            "",
            *need,
            avg_cost,
            round2(cogs),
            cost_account,
            "152",
            "",
            0.0,
            0.0,
            unit_code,
            "XuatNVL",
            &note,
            0.0,
        )
        .await?;
        // Giá vốn FIFO thực tế của phần NVL đã xuất → lưu vào snapshot của lô
        // để màn thành phần đối chiếu được giá khai và giá vốn.
        sqlx::query!(
            "UPDATE production_lot_component
                SET unit_cost = ?, cost_amount = ROUND(quantity * ?, 2)
              WHERE inbound_voucher_id = ? AND material_code = ? AND is_service = 0",
            avg_cost,
            avg_cost,
            inbound_id,
            code
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(px_no)
}

/// Lõi nhập kho (không phụ thuộc Tauri State → test trực tiếp):
/// tạo journal_entry (PN) + stock_lot (FIFO) trong 1 transaction.
/// Theo TT 88/2021/TT-BTC (mẫu 03-VT), Phiếu nhập kho áp dụng cho MỌI trường
/// hợp hàng vào kho: mua ngoài (purchase), tự sản xuất / thuê gia công
/// (production), nhập khác (other — thừa kiểm kê, điều chỉnh...).
#[allow(clippy::too_many_arguments)]
/// Lưu phiếu nhập trong 1 transaction riêng (đường dùng cũ: chỉ còn test).
#[cfg(test)]
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
    auto_bom: bool,
) -> Result<InboundResult, String> {
    save_inbound_core_with(
        pool,
        posting_date,
        voucher_no,
        description,
        supplier_code,
        warehouse_code,
        unit_code,
        items,
        note,
        inbound_type,
        reference_no,
        vat_rate,
        debit_account,
        credit_account,
        pay_now,
        adjust_dir,
        auto_bom,
        &InboundExtra::default(),
    )
    .await
}

/// Như [`save_inbound_core`] nhưng kèm nguồn phiếu + hóa đơn gốc.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_inbound_core_with(
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
    auto_bom: bool,
    extra: &InboundExtra<'_>,
) -> Result<InboundResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let r = save_inbound_core_tx(
        &mut tx,
        posting_date,
        voucher_no,
        description,
        supplier_code,
        warehouse_code,
        unit_code,
        items,
        note,
        inbound_type,
        reference_no,
        vat_rate,
        debit_account,
        credit_account,
        pay_now,
        adjust_dir,
        auto_bom,
        extra,
    )
    .await?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(r)
}

/// Thân lưu phiếu nhập, nhận transaction từ ngoài để caller có thể xoá phiếu
/// nhập tay cũ + ghi đè + neo hóa đơn trong CÙNG một transaction.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_inbound_core_tx(
    tx: &mut SqliteTransaction<'_>,
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
    // Bật "Tự xuất NVL theo định mức" — chỉ có ý nghĩa với loại nhập
    // `production` (màn PNK có ô tick). HĐĐT đồng bộ / phiếu nhập khác → false.
    auto_bom: bool,
    extra: &InboundExtra<'_>,
) -> Result<InboundResult, String> {
    let voucher_no = voucher_no.trim();
    if voucher_no.is_empty() {
        return Err("Cần nhập số phiếu".into());
    }
    let source = if extra.source.trim().is_empty() {
        "manual"
    } else {
        extra.source.trim()
    };

    // Số phiếu nhập đã có rồi thì chặn: bảng đầu phiếu có UNIQUE nên nếu không
    // chặn, lần lưu sau sẽ bị `ON CONFLICT DO NOTHING` nuốt mất và gộp bút toán
    // vào phiếu cũ — HĐĐT đồng bộ cũng sẽ neo nhầm vào phiếu đó.
    // Khi overwrite, caller đã xác định đúng phiếu cần ghi đè nên bỏ chốt này.
    if !extra.overwrite && voucher_no_taken(&mut *tx, "PN", voucher_no).await? {
        return Err(format!("Số phiếu nhập {voucher_no} đã tồn tại"));
    }

    // Chặn 2 phiếu nhập cùng ký hiệu + số + ngày hóa đơn: đồng bộ HĐĐT không
    // biết chọn phiếu nào để ghi đè, còn user thì dễ nhập trùng.
    let inv_serial = extra.invoice_serial.trim();
    let inv_no = extra.invoice_no.trim();
    let inv_date = extra.invoice_date.trim();
    if inbound_type == "purchase"
        && !inv_serial.is_empty()
        && !inv_no.is_empty()
        && !inv_date.is_empty()
    {
        let exclude_id = extra.exclude_id.unwrap_or(-1);
        let dup: Option<String> = sqlx::query_scalar!(
            r#"SELECT voucher_no FROM inbound_voucher
                WHERE id <> ?
                  AND inbound_type = 'purchase'
                  AND lower(trim(invoice_serial)) = lower(trim(?))
                  AND lower(trim(invoice_no)) = lower(trim(?))
                  AND invoice_date = ?
                LIMIT 1"#,
            exclude_id,
            inv_serial,
            inv_no,
            inv_date
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        if let Some(voucher) = dup {
            return Err(format!(
                "Hóa đơn {inv_serial}-{inv_no} ngày {inv_date} đã có trong phiếu nhập {voucher}. \
                 Mỗi hóa đơn chỉ được nhập 1 phiếu — xem lại phiếu cũ trước khi lưu."
            ));
        }
    }

    // Hộ có được khấu trừ GTGT đầu vào không (app_setting 'vat_deduct')?
    //   Bật  → tách thuế: Nợ 133 / Có 331, giá vốn = giá chưa thuế.
    //   Tắt  → thuế nằm trong giá vốn: Nợ 152 (gồm thuế) / Có 331, không ghi 133.
    let vat_deduct: String = sqlx::query_scalar!(
        r#"SELECT COALESCE((SELECT value FROM app_setting WHERE key = 'vat_deduct'), '0') as "v!: String""#
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let vat_deductible = vat_deduct == "1";

    // Đầu phiếu nhập (inbound_voucher) — id này là điểm neo để hóa đơn chính
    // thức liên kết chặt với phiếu nhập. Tạo TRƯỚC các dòng bút toán để
    // gắn inbound_voucher_id ngay khi ghi.
    sqlx::query!(
        "INSERT INTO inbound_voucher
            (voucher_no, posting_date, supplier_code, description, reference_no,
             inbound_type, warehouse_code, unit_code, vat_rate, source, note,
             invoice_serial, invoice_no, invoice_date, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?, datetime('now'))
         ON CONFLICT(voucher_no) DO NOTHING",
        voucher_no,
        posting_date,
        supplier_code,
        description,
        reference_no,
        inbound_type,
        warehouse_code,
        unit_code,
        vat_rate,
        source,
        inv_serial,
        inv_no,
        inv_date
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let inbound_header_id: i64 = sqlx::query_scalar!(
        r#"SELECT id as "id!" FROM inbound_voucher WHERE voucher_no = ?"#,
        voucher_no
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    // Chụp định mức hiện hành NGAY khi lưu lô sản xuất. Bảng định mức được sửa
    // tự do, nhưng thành phần của lô đã sản xuất phải giữ nguyên để đối chiếu.
    // Phiếu được ghi đè (overwrite) thì xoá snapshot cũ trước khi chụp lại.
    let production_lines = if inbound_type == "production" {
        let lines = load_production_bom_lines(&mut *tx, items, warehouse_code).await?;
        snapshot_production_components(&mut *tx, inbound_header_id, &lines).await?;
        lines
    } else {
        Vec::new()
    };

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
    // Tổng giá trị hàng (sau CK, chưa thuế) + tổng thuế gộp — dùng để lưu
    // `vat_rate` đầu phiếu dạng tỷ lệ bình quân gia quyền cho báo cáo cũ.
    let mut net_total = 0.0;
    let mut gross_vat = 0.0;
    // Số chứng từ gốc (hóa đơn / lệnh nhập kho — cột "Theo..." của mẫu 03-VT)
    // chuyển vào cột ghi chú của bút toán để truy vết được chứng từ gốc.
    let note_full = if reference_no.trim().is_empty() {
        note.to_string()
    } else if note.trim().is_empty() {
        format!("Theo chứng từ: {}", reference_no.trim())
    } else {
        format!("Theo chứng từ: {} · {}", reference_no.trim(), note)
    };

    for (item_index, item) in items.iter().enumerate() {
        // Lô sản xuất được định giá theo đơn giá KHAI trên từng thành phần định
        // mức (kể cả dịch vụ) — không phụ thuộc giá vốn FIFO của NVL. Chưa khai
        // giá (tổng = 0) thì giữ đơn giá người dùng nhập như trước.
        let unit_price = if inbound_type == "production" {
            declared_unit_price(&production_lines, item_index, item.quantity)
                .unwrap_or(item.unit_price)
        } else {
            item.unit_price
        };
        // Giá trị nhập kho = Thành tiền − Tiền CK (chiết khấu thương mại).
        // VAT đầu vào tính trên giá sau chiết khấu (CK được trừ vào giá tính thuế).
        let gross = round2(item.quantity * unit_price);
        // Tiền CK không thể vượt quá thành tiền (tránh giá trị âm). Hóa đơn
        // điều chỉnh giảm có thể có gross âm — `clamp(0, gross)` sẽ panic.
        let discount = if gross >= 0.0 {
            round2(item.discount.clamp(0.0, gross))
        } else {
            round2(item.discount)
        };
        let net = round2(gross - discount);
        // Thuế suất của RIÊNG dòng này (mỗi mặt hàng có thể khác mức). Dữ liệu
        // cũ / phiếu không gửi lên → rơi về mức chung đầu phiếu.
        let line_vat_rate = item.vat_rate.unwrap_or(vat_rate).clamp(0.0, 1.0);
        // HĐĐT gửi số thuế tuyệt đối đã phân bổ để tổng khớp đúng hóa đơn;
        // phiếu nhập tay chỉ có tỷ lệ → tính trên giá sau chiết khấu.
        let vat = match item.vat_amount {
            Some(amount) => round2(amount),
            None if line_vat_rate > 0.0 => round2(net * line_vat_rate),
            None => 0.0,
        };
        // Giá trị nhập kho / giá vốn: hộ khấu trừ thì là giá chưa thuế; hộ không
        // khấu trừ thì thuế nằm luôn trong giá vốn (không sinh bút toán 133).
        let cost_value = if vat_deductible {
            net
        } else {
            round2(net + vat)
        };
        // Đơn giá thực tế sau chiết khấu (+ thuế nếu không khấu trừ) — giá vốn FIFO.
        let unit_cost = if item.quantity.abs() > 1e-9 {
            round2(cost_value / item.quantity)
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
            let wh = resolve_warehouse(&mut *tx, warehouse_code, &item.product_code).await?;
            let product_id = resolve_product(&mut *tx, &item.product_code).await?.id;
            let lots: Vec<(i64, f64, f64)> = sqlx::query!(
                r#"SELECT id as "id!", quantity, unit_cost FROM stock_lot
                   WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
                   ORDER BY received_at ASC, id ASC"#,
                product_id,
                wh
            )
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| (r.id, r.quantity, r.unit_cost))
            .collect();
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
                .execute(&mut **tx)
                .await
                .map_err(|e| e.to_string())?;
            }
            insert_journal_entry(
                &mut *tx,
                posting_date,
                voucher_no,
                "PN",
                description,
                &item.product_code,
                supplier_code,
                "",
                item.quantity,
                unit_cost,
                cost_value,
                credit,
                debit,
                "",
                line_vat_rate,
                0.0,
                unit_code,
                "GiamCP",
                &note_full,
                // Dòng điều chỉnh giảm hàng + chiết phí: tiền CK gắn với dòng này.
                discount,
            )
            .await?;
            if vat_deductible && vat.abs() > 0.0 {
                insert_journal_entry(
                    &mut *tx,
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
                    line_vat_rate,
                    0.0,
                    unit_code,
                    "GiamCP",
                    &note_full,
                    0.0,
                )
                .await?;
            }
            net_total += net;
            gross_vat += vat;
            total += cost_value;
            vat_total += if vat_deductible { vat } else { 0.0 };
            continue;
        }

        // ─── Nhập kho thường (mua ngoài / tự sản xuất / điều chỉnh tăng / khác) ───
        let wh_id = resolve_warehouse(&mut *tx, warehouse_code, &item.product_code).await?;
        let product_id = resolve_product(&mut *tx, &item.product_code).await?.id;
        // Đơn giá nhập vừa giao làm GIÁ VỐN HIỆN TẠI của mặt hàng — lần nhập sau
        // lấy làm mặc định (giá 0 không xoá giá đã lưu). Nhập kho không đụng giá
        // bán — giá bán chỉ nhớ từ luồng bán (lập hóa đơn / xuất bán).
        // `unit_cost` đã là giá sau chiết khấu (kèm thuế nếu hộ không khấu trừ)
        // nên dùng thẳng làm giá vốn mặc định — không nhân thuế lần thứ hai.
        let default_cost = unit_cost;
        if default_cost > 0.0 {
            sqlx::query!(
                "UPDATE product SET cost_price = ? WHERE id = ?",
                default_cost,
                product_id
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        // tạo stock_lot FIFO — giá vốn = đơn giá thực tế sau chiết khấu
        // (chưa thuế khi khấu trừ; gồm thuế khi không khấu trừ)
        sqlx::query!(
            "INSERT INTO stock_lot
                (product_id, warehouse_id, quantity, unit_cost, received_at, depleted, inbound_voucher_id)
             VALUES (?, ?, ?, ?, ?, 0, ?)",
            product_id,
            wh_id,
            item.quantity,
            unit_cost,
            posting_date,
            inbound_header_id
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        // ghi sổ nhập — giá trị nhập kho = Thành tiền − Tiền CK
        // (chưa thuế khi khấu trừ; gồm thuế khi không khấu trừ)
        insert_journal_entry(
            &mut *tx,
            posting_date,
            voucher_no,
            "PN",
            description,
            &item.product_code,
            supplier_code,
            "",
            item.quantity,
            unit_cost,
            cost_value,
            debit,
            credit,
            "",
            line_vat_rate,
            0.0,
            unit_code,
            "",
            &note_full,
            discount,
        )
        .await?;
        // Khấu trừ GTGT: bút toán bổ sung Nợ 133 (thuế được khấu trừ) / Có credit.
        // Hộ không khấu trừ thì thuế đã nằm trong cost_value ở trên → không ghi 133.
        // Số lượng 0 để không ảnh hưởng tồn kho / báo cáo số lượng.
        if vat_deductible && vat.abs() > 0.0 {
            insert_journal_entry(
                &mut *tx,
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
                line_vat_rate,
                0.0,
                unit_code,
                "",
                &note_full,
                0.0,
            )
            .await?;
        }
        net_total += net;
        gross_vat += vat;
        total += cost_value;
        vat_total += if vat_deductible { vat } else { 0.0 };
    }

    // Công tắc "Trả tiền ngay" (mặc định BẬT): mua hàng ngoài có nhà cung cấp → tự tạo
    // phiếu chi (PC) thanh toán trong cùng transaction: Nợ TK đối ứng của PNK (mặc định
    // 331 — phải trả người bán) / Có 111 tiền mặt. Số tiền = Giá trị nhập kho + Thuế
    // GTGT đầu vào khấu trừ (nếu có). Không tạo khi TK Có PNK đã là tiền mặt /
    // ngân hàng (hàng đã trả ngay ngay trên PNK) hoặc chưa chọn nhà cung cấp.
    let mut pc_no = String::new();
    let credit_is_cash = credit == "111" || credit == "112";
    if pay_now && inbound_type == "purchase" && !supplier_code.trim().is_empty() && !credit_is_cash
    {
        let pay = round2(total + vat_total);
        if pay > 0.0 {
            pc_no = next_pc_no(&mut *tx).await?;
            let desc = if description.trim().is_empty() {
                "Trả tiền mua hàng".to_string()
            } else {
                format!("Trả tiền mua hàng — {}", description.trim())
            };
            // Liên kết phiếu chi với PNK qua số chứng từ gốc + mã nhà cung cấp.
            let pc_note = format!("Theo chứng từ: {}", voucher_no);
            insert_journal_entry(
                &mut *tx,
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
                0.0,
            )
            .await?;
        }
    }

    // F4 — Tự xuất NVL theo định mức (chỉ loại nhập sản xuất + người dùng bật ô
    // tick). Chạy NGAY TRONG transaction này: thiếu tồn NVL thì `?` làm roll back
    // CẢ phiếu nhập → không bao giờ có PNK đã lưu mà NVL chưa trừ.
    let mut material_voucher = String::new();
    if auto_bom && inbound_type == "production" {
        material_voucher = issue_materials_by_bom(
            &mut *tx,
            inbound_header_id,
            posting_date,
            voucher_no,
            warehouse_code,
            unit_code,
            credit,
            &production_lines,
        )
        .await?;
    }

    // Gắn id đầu phiếu vào các dòng bút toán của chính phiếu này + chốt tổng.
    // (Lọc theo voucher_no + entry_type để không đụng phiếu khác trùng số.)
    sqlx::query!(
        "UPDATE journal_entry SET inbound_voucher_id = ?
          WHERE voucher_no = ? AND entry_type = 'PN'",
        inbound_header_id,
        voucher_no
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    // Tỷ lệ thuế bình quân gia quyền chỉ để hiển thị/báo cáo cũ; bút toán và
    // tổng thuế đã lấy theo từng dòng ở trên.
    let header_vat_rate = if net_total.abs() > 1e-9 {
        (gross_vat / net_total * 10_000.0).round() / 10_000.0
    } else {
        0.0
    };
    sqlx::query!(
        "UPDATE inbound_voucher SET total = ?, vat_amount = ?, reference_no = ?,
                posting_date = ?, supplier_code = ?, description = ?, unit_code = ?,
                warehouse_code = ?, vat_rate = ?, source = ?,
                invoice_serial = ?, invoice_no = ?, invoice_date = ?
          WHERE id = ?",
        total,
        vat_total,
        reference_no,
        posting_date,
        supplier_code,
        description,
        unit_code,
        warehouse_code,
        header_vat_rate,
        source,
        inv_serial,
        inv_no,
        inv_date,
        inbound_header_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    Ok(InboundResult {
        inbound_voucher_id: inbound_header_id,
        entries: items.len(),
        total,
        vat: vat_total,
        pc_no,
        material_voucher,
    })
}

/// Phiếu nhập tay khớp với 1 hóa đơn HĐĐT (ký hiệu + số + ngày, hoặc dự phòng
/// theo `reference_no` đã chuẩn hoá + ngày hạch toán).
#[derive(Debug, Clone)]
pub(crate) struct MatchedInbound {
    pub(crate) id: i64,
    pub(crate) voucher_no: String,
    pub(crate) posting_date: String,
    pub(crate) warehouse_code: String,
}

/// Bỏ khoảng trắng/dấu gạch để so "C26TAA-000123" với "C26TAA000123".
fn normalize_invoice_ref(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Tìm phiếu nhập tay cần ghi đè khi đồng bộ hóa đơn. Trả lỗi nếu có >1 phiếu
/// cùng khớp — không đoán bừa để tránh ghi đè sai chứng từ.
pub(crate) async fn find_matching_inbound(
    pool: &SqlitePool,
    serial: &str,
    number: &str,
    date: &str,
) -> Result<Option<MatchedInbound>, String> {
    let serial = serial.trim();
    let number = number.trim();
    let date = date.trim();
    if date.is_empty() {
        return Ok(None);
    }
    let want_ref = normalize_invoice_ref(&format!("{serial}{number}"));
    let rows = sqlx::query!(
        r#"SELECT id as "id!", voucher_no, posting_date, warehouse_code,
                  invoice_serial, invoice_no, invoice_date, reference_no
             FROM inbound_voucher
            WHERE inbound_type = 'purchase'
              AND source IN ('manual', 'legacy')
              AND NOT EXISTS (
                    SELECT 1 FROM hddt_imported_invoice hi
                     WHERE hi.inbound_voucher_id = inbound_voucher.id
              )
              AND (
                    (invoice_serial <> '' AND invoice_no <> '' AND invoice_date <> ''
                     AND invoice_date = ?)
                 OR (reference_no <> '' AND posting_date = ?)
              )
            ORDER BY id"#,
        date,
        date
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut found: Vec<MatchedInbound> = Vec::new();
    for r in rows {
        let explicit = !r.invoice_serial.trim().is_empty()
            && !r.invoice_no.trim().is_empty()
            && !r.invoice_date.trim().is_empty()
            && r.invoice_date.trim() == date
            && r.invoice_serial.trim().eq_ignore_ascii_case(serial)
            && r.invoice_no.trim().eq_ignore_ascii_case(number);
        let fallback = r.invoice_serial.trim().is_empty()
            && r.invoice_no.trim().is_empty()
            && r.posting_date.trim() == date
            && !want_ref.is_empty()
            && normalize_invoice_ref(&r.reference_no) == want_ref;
        if explicit || fallback {
            found.push(MatchedInbound {
                id: r.id,
                voucher_no: r.voucher_no,
                posting_date: r.posting_date,
                warehouse_code: r.warehouse_code,
            });
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => {
            let names = found
                .iter()
                .map(|m| m.voucher_no.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Có {} phiếu nhập cùng khớp hóa đơn {serial}-{number} ngày {date}: {names}. \
                 Hãy sửa/xoá bớt để chỉ còn 1 phiếu trước khi đồng bộ.",
                found.len()
            ))
        }
    }
}

/// Xoá bút toán PN + lô tồn kho của phiếu nhập tay cũ để ghi đè bằng dữ liệu
/// hóa đơn. Chỉ xoá khi TOÀN BỘ lô còn nguyên (chưa xuất/điều chỉnh) — nếu lô
/// đã dùng, trả lỗi để không nhân đôi tồn kho hoặc làm lệch giá vốn đã bán.
pub(crate) async fn clear_inbound_for_overwrite(
    tx: &mut SqliteTransaction<'_>,
    matched: &MatchedInbound,
) -> Result<(), String> {
    let linked: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM hddt_imported_invoice WHERE inbound_voucher_id = ?",
        matched.id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    if linked > 0 {
        return Err(format!(
            "Phiếu {} đã liên kết hóa đơn khác — không ghi đè.",
            matched.voucher_no
        ));
    }

    let entries = sqlx::query!(
        r#"SELECT product_code,
                  SUM(quantity) as "quantity!: f64",
                  unit_price
             FROM journal_entry
            WHERE inbound_voucher_id = ? AND entry_type = 'PN' AND product_code <> ''
            GROUP BY product_code, unit_price"#,
        matched.id
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    let mut lot_ids: Vec<i64> = Vec::new();
    for e in &entries {
        let product = resolve_product(&mut *tx, &e.product_code).await?;
        let wh_id = resolve_warehouse(&mut *tx, &matched.warehouse_code, &e.product_code).await?;
        // Lô do chính phiếu này tạo (dữ liệu mới) — ưu tiên.
        let mut lots: Vec<(i64, f64)> = sqlx::query!(
            r#"SELECT id as "id!", quantity
                 FROM stock_lot
                WHERE product_id = ? AND warehouse_id = ?
                  AND inbound_voucher_id = ?
                  AND ABS(unit_cost - ?) < 0.005
                  AND depleted = 0
                ORDER BY id"#,
            product.id,
            wh_id,
            matched.id,
            e.unit_price
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.id, r.quantity))
        .collect();
        // Phiếu cũ chưa có inbound_voucher_id: dò theo ngày + giá vốn. Chỉ nhận
        // khi tổng số lượng khớp CHÍNH XÁC phần bút toán của phiếu — tránh xoá
        // nhầm lô của phiếu khác cùng ngày.
        if lots.is_empty() {
            lots = sqlx::query!(
                r#"SELECT id as "id!", quantity
                     FROM stock_lot
                    WHERE product_id = ? AND warehouse_id = ?
                      AND received_at = ?
                      AND ABS(unit_cost - ?) < 0.005
                      AND inbound_voucher_id IS NULL
                      AND depleted = 0
                    ORDER BY id"#,
                product.id,
                wh_id,
                matched.posting_date,
                e.unit_price
            )
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| (r.id, r.quantity))
            .collect();
        }
        let sum: f64 = lots.iter().map(|l| l.1).sum();
        if (sum - e.quantity).abs() > 0.005 {
            return Err(format!(
                "Phiếu {} đã bị xuất/điều chỉnh kho (mặt hàng {}, cần {:.2}, còn {:.2}) — \
                 không ghi đè được. Hãy xoá/sửa phiếu cũ hoặc nhập hóa đơn thành phiếu mới.",
                matched.voucher_no, e.product_code, e.quantity, sum
            ));
        }
        lot_ids.extend(lots.iter().map(|l| l.0));
    }

    // Lô còn neo vào phiếu nhưng không khớp bút toán nào → dữ liệu lạ, dừng lại.
    let linked_lots = sqlx::query_scalar!(
        r#"SELECT id as "id!" FROM stock_lot WHERE inbound_voucher_id = ?"#,
        matched.id
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(orphan) = linked_lots.iter().find(|id| !lot_ids.contains(id)) {
        return Err(format!(
            "Phiếu {} có lô tồn kho không khớp bút toán (lô #{}) — không ghi đè được.",
            matched.voucher_no, orphan
        ));
    }

    for id in &lot_ids {
        sqlx::query!("DELETE FROM stock_lot WHERE id = ?", id)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    sqlx::query!(
        "DELETE FROM journal_entry WHERE inbound_voucher_id = ? AND entry_type = 'PN'",
        matched.id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Số lớn nhất trong danh sách số phiếu (bỏ ký tự không phải số ở đuôi).
fn max_voucher_seq(rows: &[String]) -> i64 {
    rows.iter()
        .filter_map(|v| {
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
        .unwrap_or(0)
}

/// Số phiếu kế tiếp theo loại chứng từ (PN0001 / PX001 / PT001 / PC001).
///
/// Sinh từ CƠ SỞ DỮ LIỆU chứ không đoán ở màn hình: danh sách trên UI có thể
/// đang lọc (tìm kiếm / khoảng ngày) nên suy ra số từ danh sách hiển thị sẽ
/// đụng số phiếu đang tồn tại.
pub(crate) async fn next_voucher_no_core(
    pool: &SqlitePool,
    entry_type: &str,
) -> Result<String, String> {
    let (rows, prefix, pad): (Vec<String>, &str, usize) = match entry_type {
        "PN" => (
            sqlx::query_scalar!("SELECT voucher_no FROM inbound_voucher")
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?,
            "PN",
            4,
        ),
        "PX" | "PT" | "PC" => (
            sqlx::query_scalar!(
                "SELECT voucher_no FROM journal_entry WHERE entry_type = ?",
                entry_type
            )
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?,
            match entry_type {
                "PX" => "PX",
                "PT" => "PT",
                _ => "PC",
            },
            3,
        ),
        other => return Err(format!("Loại chứng từ '{other}' không sinh số phiếu được")),
    };
    let next = max_voucher_seq(&rows) + 1;
    Ok(format!("{prefix}{next:0pad$}"))
}

#[tauri::command]
pub(crate) async fn next_voucher_no(
    state: State<'_, AppState>,
    entry_type: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    next_voucher_no_core(&*state.pool.read().await, &entry_type).await
}

/// Số phiếu chi (PC) tiếp theo: lấy số lớn nhất đang có + 1 (PC001, PC002…).
/// Khớp với quy ước gợi ý số phiếu ở màn Phiếu thu/chi.
async fn next_pc_no(tx: &mut SqliteTransaction<'_>) -> Result<String, String> {
    let rows: Vec<String> =
        sqlx::query_scalar!("SELECT voucher_no FROM journal_entry WHERE entry_type = 'PC'")
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    Ok(format!("PC{:03}", max_voucher_seq(&rows) + 1))
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
    let rows: Vec<String> =
        sqlx::query_scalar!("SELECT voucher_no FROM journal_entry WHERE entry_type = 'PT'")
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    Ok(format!("PT{:03}", max_voucher_seq(&rows) + 1))
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
    // (mã SP, SL, ĐG, nhóm ngành, thuế GTGT, thuế TNCN, TÊN hiển thị dòng)
    let mut invoice_lines: Vec<(i64, f64, f64, String, f64, f64, String)> = Vec::new();

    // Điều chỉnh GIẢM hóa đơn bán (khách trả lại / giảm doanh thu): ghi đảo doanh
    // thu và nhập lại hàng về kho, KHÔNG xuất FIFO và không tạo phiếu thu.
    let voucher_no = voucher_no.trim();
    if voucher_no.is_empty() {
        return Err("Cần nhập số phiếu".into());
    }
    // Trùng số phiếu xuất = hai chứng tờ khác nhau cùng một số → sổ in ra bốn
    // chứng từ trùng số, không tra cứu được. Chặn trước khi ghi bất kỳ dòng nào.
    if voucher_no_taken(&mut tx, "PX", voucher_no).await? {
        return Err(format!("Số phiếu xuất {voucher_no} đã tồn tại"));
    }

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

        // Ghi sổ bán hàng: doanh thu dòng = SL × Đơn giá − Tiền CK (chiết khấu).
        let amount = round2(item.quantity * item.unit_price - item.discount);

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
                    item.line_name.clone(),
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
                item.discount,
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
                0.0,
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
                0.0,
            )
            .await?;
            revenue_total += amount;
            continue;
        }

        // Kho xuất trên dòng: rỗng → kho mặc định của sản phẩm / kho đầu tiên.
        let wh_id = resolve_warehouse(&mut tx, &item.warehouse_code, &item.product_code).await?;

        // Hàng hóa: FIFO lấy các lô chưa xuất hết theo ngày nhập, trong đúng kho đã chọn
        let lots: Vec<(i64, f64, f64)> = sqlx::query!(
            r#"SELECT id as "id!", quantity, unit_cost FROM stock_lot
               WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
               ORDER BY received_at ASC, id ASC"#,
            product.id,
            wh_id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.id, r.quantity, r.unit_cost))
        .collect();

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
            0.0,
        )
        .await?;
        // Giá vốn của đúng số hàng vừa xuất (theo FIFO). Ghi cặp Nợ 632 / Có 152
        // để bảng cân đối có cột giá vốn và lợi nhuận trước thuế không bị tính
        // thừa — hàng bán ra trước đây không hiện ở cột chi phí nào. Phiếu điều
        // chỉnh giảm (khách trả lại) đã ghi đảo cặp này ở nhánh trên.
        if cogs > 0.0 && item.quantity > 0.0 {
            insert_journal_entry(
                &mut tx,
                posting_date,
                voucher_no,
                "CP",
                description,
                &item.product_code,
                "",
                customer_code,
                item.quantity,
                cogs / item.quantity,
                round2(cogs),
                "632",
                "152",
                &industry,
                0.0,
                0.0,
                unit_code,
                "",
                note,
                0.0,
            )
            .await?;
        }
        invoice_lines.push((
            product.id,
            item.quantity,
            item.unit_price,
            industry.clone(),
            vat_rate,
            pit_rate,
            item.line_name.clone(),
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
                0.0,
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
        for (pid, qty, price, ind, vat, pit, line_name) in &invoice_lines {
            let subtotal = qty * price;
            let (pid, qty, price) = (*pid, *qty, *price);
            let (ind, vat, pit) = (ind.clone(), *vat, *pit);
            sqlx::query!(
                "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                           industry_code, vat_rate, pit_rate, line_name)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                inv_id,
                pid,
                qty,
                price,
                subtotal,
                ind,
                vat,
                pit,
                line_name
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            // Giá bán vừa ghi trên phiếu xuất → giá bán hiện tại của mặt hàng,
            // lần sau chọn mặt hàng là tự lấy đúng giá này (giá 0 giữ nguyên giá cũ).
            if price > 0.0 {
                sqlx::query!("UPDATE product SET sale_price = ? WHERE id = ?", price, pid)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            }
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
/// vat_rate (%): bật app_setting 'vat_deduct' (= '1') thì tách Nợ 133 (khấu trừ);
/// tắt thì cộng thuế vào giá trị nhập kho / giá vốn (không ghi 133).
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
    // Ô tick "Tự xuất NVL theo định mức" của màn PNK sản xuất.
    auto_bom: bool,
    // Ký hiệu + số + ngày hóa đơn (chỉ phiếu mua hàng): dùng để nhận diện
    // phiếu nhập tay trùng hóa đơn HĐĐT và chặn nhập trùng.
    invoice_serial: Option<String>,
    invoice_no: Option<String>,
    invoice_date: Option<String>,
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
    // Công tắc "khấu trừ GTGT đầu vào" (mặc định TẮT): bật thì core tách 133,
    // tắt thì core cộng thuế vào giá vốn (hộ nộp thuế theo doanh thu).
    let pool = state.pool.read().await;
    let vat_deduct: String = sqlx::query_scalar!(
        r#"SELECT COALESCE((SELECT value FROM app_setting WHERE key = 'vat_deduct'), '0') as "v!: String""#
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| e.to_string())?;

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
    // Chỉ cần TK 133 khi thực sự tách thuế khấu trừ; hộ không khấu trừ cộng thuế
    // vào giá vốn nên không phụ thuộc tài khoản 133.
    if vat_deduct == "1" && items.iter().any(|it| it.vat_rate.unwrap_or(vat_rate) > 0.0) {
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
        let exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM account WHERE code = ?", code)
            .fetch_one(&*pool)
            .await
            .map_err(|e| e.to_string())?;
        if exists == 0 {
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
    let invoice_serial = invoice_serial.unwrap_or_default().trim().to_string();
    let invoice_no = invoice_no.unwrap_or_default().trim().to_string();
    let invoice_date = invoice_date.unwrap_or_default().trim().to_string();
    let extra = InboundExtra {
        source: "manual",
        invoice_serial: &invoice_serial,
        invoice_no: &invoice_no,
        invoice_date: &invoice_date,
        overwrite: false,
        exclude_id: None,
    };
    let r = save_inbound_core_with(
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
        auto_bom,
        &extra,
    )
    .await?;
    audit(&state, "save", "inbound", &voucher_no).await;
    Ok(json!({
        "ok": true,
        "entries": r.entries,
        "total": r.total,
        "vat": r.vat,
        "pc_no": r.pc_no,
        "material_voucher": r.material_voucher,
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
        let exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM account WHERE code = '111'")
            .fetch_one(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
        if exists == 0 {
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
    search: String,
) -> Result<String, String> {
    let rows = get_journal_entries_core(
        &*state.pool.read().await,
        &entry_type,
        &from_date,
        &to_date,
        &search,
    )
    .await?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Phiếu thu/chi — phân trang server-side (màn Thu/Chi không lọc theo ngày nên
/// danh sách phình to theo thời gian).
#[tauri::command]
pub(crate) async fn get_journal_entries_page(
    state: State<'_, AppState>,
    lazy_event: String,
    entry_type: String,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Ô tìm kiếm toàn cục của bảng đóng vai trò `search` của bản get_journal_entries.
    let search = ev.global_keyword().unwrap_or_default();
    let rows = get_journal_entries_page_core(
        &*state.pool.read().await,
        &entry_type,
        &from_date,
        &to_date,
        &search,
        page.size,
        page.offset,
    )
    .await?;
    let total = count_journal_entries(
        &*state.pool.read().await,
        &entry_type,
        &from_date,
        &to_date,
        &search,
    )
    .await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows, total })
            .unwrap_or_default(),
    )
}

/// Đếm số dòng khớp bộ lọc — để bảng tính tổng số trang.
pub(crate) async fn count_journal_entries(
    pool: &SqlitePool,
    entry_type: &str,
    from_date: &str,
    to_date: &str,
    search: &str,
) -> Result<i64, String> {
    let search = search.trim();
    let like = format!("%{search}%");
    let total = sqlx::query_scalar!(
        "SELECT COUNT(*)
         FROM journal_entry je
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         LEFT JOIN product pr ON pr.code = je.product_code
         WHERE (? = '' OR je.entry_type = ?)
           AND (? = '' OR je.posting_date >= ?)
           AND (? = '' OR je.posting_date <= ?)
           AND (? = ''
                OR je.voucher_no LIKE ? COLLATE NOCASE
                OR je.product_code LIKE ? COLLATE NOCASE
                OR je.description LIKE ? COLLATE NOCASE
                OR je.note LIKE ? COLLATE NOCASE
                OR COALESCE(s.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(c.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(pr.name, '') LIKE ? COLLATE NOCASE)",
        entry_type,
        entry_type,
        from_date,
        from_date,
        to_date,
        to_date,
        search,
        like,
        like,
        like,
        like,
        like,
        like,
        like
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(total)
}

/// Sổ nhật ký theo bộ lọc — tách riêng để test trực tiếp (không cần tauri::State).
///
/// Từ khoá tìm khớp trên số phiếu / mã hàng / tên khách, nhà cung cấp / diễn
/// giải / ghi chú. Danh sách sắp theo ngày nên phiếu có ngày cũ nằm giữa danh
/// sách — cần đường tìm nhanh thay vì lần theo từng trang.
pub(crate) async fn get_journal_entries_core(
    pool: &SqlitePool,
    entry_type: &str,
    from_date: &str,
    to_date: &str,
    search: &str,
) -> Result<Vec<JournalEntryRow>, String> {
    let search = search.trim();
    let like = format!("%{search}%");
    let rows: Vec<JournalEntryRow> = sqlx::query_as!(
        JournalEntryRow,
        "SELECT je.id, je.posting_date, je.voucher_no, je.entry_type, je.description,
                je.product_code, je.quantity, je.unit_price, je.amount,
                s.name AS supplier_name, c.name AS customer_name,
                je.industry_code, je.adjust_code, je.note
         FROM journal_entry je
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         LEFT JOIN product pr ON pr.code = je.product_code
         WHERE (? = '' OR je.entry_type = ?)
           AND (? = '' OR je.posting_date >= ?)
           AND (? = '' OR je.posting_date <= ?)
           AND (? = ''
                OR je.voucher_no LIKE ? COLLATE NOCASE
                OR je.product_code LIKE ? COLLATE NOCASE
                OR je.description LIKE ? COLLATE NOCASE
                OR je.note LIKE ? COLLATE NOCASE
                OR COALESCE(s.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(c.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(pr.name, '') LIKE ? COLLATE NOCASE)
         ORDER BY je.posting_date DESC, je.id DESC
         LIMIT 500",
        entry_type,
        entry_type,
        from_date,
        from_date,
        to_date,
        to_date,
        search,
        like,
        like,
        like,
        like,
        like,
        like,
        like
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Bản phân trang của `get_journal_entries_core`: cùng bộ lọc, thêm LIMIT/OFFSET
/// và sắp xếp theo yêu cầu của bảng (mặc định mới nhất trước).
pub(crate) async fn get_journal_entries_page_core(
    pool: &SqlitePool,
    entry_type: &str,
    from_date: &str,
    to_date: &str,
    search: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<JournalEntryRow>, String> {
    let search = search.trim();
    let like = format!("%{search}%");
    let rows: Vec<JournalEntryRow> = sqlx::query_as!(
        JournalEntryRow,
        "SELECT je.id, je.posting_date, je.voucher_no, je.entry_type, je.description,
                je.product_code, je.quantity, je.unit_price, je.amount,
                s.name AS supplier_name, c.name AS customer_name,
                je.industry_code, je.adjust_code, je.note
         FROM journal_entry je
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         LEFT JOIN product pr ON pr.code = je.product_code
         WHERE (? = '' OR je.entry_type = ?)
           AND (? = '' OR je.posting_date >= ?)
           AND (? = '' OR je.posting_date <= ?)
           AND (? = ''
                OR je.voucher_no LIKE ? COLLATE NOCASE
                OR je.product_code LIKE ? COLLATE NOCASE
                OR je.description LIKE ? COLLATE NOCASE
                OR je.note LIKE ? COLLATE NOCASE
                OR COALESCE(s.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(c.name, '') LIKE ? COLLATE NOCASE
                OR COALESCE(pr.name, '') LIKE ? COLLATE NOCASE)
         ORDER BY je.posting_date DESC, je.id DESC
         LIMIT ? OFFSET ?",
        entry_type,
        entry_type,
        from_date,
        from_date,
        to_date,
        to_date,
        search,
        like,
        like,
        like,
        like,
        like,
        like,
        like,
        limit,
        offset
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Phiếu nhập / phiếu xuất gom theo phiếu: **mỗi phiếu 1 dòng**, phân trang ở server.
///
/// Màn kho trước đây in thẳng `journal_entry` nên một phiếu có N mặt hàng thì ra N
/// dòng, trông như N phiếu trùng số. Sổ vẫn phải giữ chi tiết dòng (dùng cho FIFO,
/// bảng cân đối, tờ khai), nên chỉ gom ở màn hiển thị; bấm vào dòng vẫn mở được
/// chi tiết từng dòng.
///
/// Cột `item_count` là SỐ LOẠI mặt hàng (`COUNT(DISTINCT product_code)`), không
/// phải tổng số lượng — đúng như bản kê trên hóa đơn. `discount` là tổng tiền chiết
/// khấu của các dòng, `amount` là tổng giá trị **sau** chiết khấu.
#[tauri::command]
pub(crate) async fn get_stock_vouchers_page(
    state: State<'_, AppState>,
    lazy_event: String,
    entry_type: String,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    let pool = state.pool.read().await;
    let search = ev.global_keyword().unwrap_or_default();
    let (rows, total) = stock_vouchers_core(
        &pool,
        &entry_type,
        &from_date,
        &to_date,
        &search,
        page.size,
        page.offset,
    )
    .await?;
    Ok(json!({ "rows": rows, "total": total }).to_string())
}

/// Mảnh SQL của điều kiện lọc: xen kẽ chữ và tham số bind.
///
/// `QueryBuilder` đặt mọi `push_bind` vào đúng vị trí đang đứng, nên điều kiện
/// và tham số phải được đẩy xen kẽ — gom hết điều kiện rồi bind sau sẽ lệch
/// chỗ và sinh SQL sai.
enum WherePart {
    Text(String),
    Bind(String),
}

/// Dựng điều kiện lọc danh sách phiếu (loại phiếu + khoảng ngày + từ khoá).
fn voucher_where_parts(
    entry_type: &str,
    from_date: &str,
    to_date: &str,
    search: &str,
) -> Vec<WherePart> {
    let mut parts = vec![WherePart::Text("1=1".to_string())];
    let mut push = |sql: &str, value: &str| {
        parts.push(WherePart::Text(sql.to_string()));
        parts.push(WherePart::Bind(value.to_string()));
    };
    if !entry_type.trim().is_empty() {
        push(" AND entry_type = ", entry_type);
    }
    if !from_date.trim().is_empty() {
        push(" AND posting_date >= ", from_date);
    }
    if !to_date.trim().is_empty() {
        push(" AND posting_date <= ", to_date);
    }
    // Từ khoá: mỗi cột một bind, xen kẽ với chữ "LIKE" để thứ tự `?` khớp.
    // (Không dùng `page::global_where` vì hàm đó gộp toàn bộ điều kiện thành
    // một chuỗi, bind sẽ rơi hết ra sau ngoặc đóng.)
    let kw = search.trim();
    if !kw.is_empty() {
        // Khoá ký tự đại diện để gõ `%` cũng chỉ tìm đúng chữ đó.
        let like = format!("%{}%", kw.replace('%', "\\%").replace('_', "\\_"));
        let cols = [
            "voucher_no",
            "description",
            "supplier_name",
            "customer_name",
            "note",
        ];
        parts.push(WherePart::Text(" AND (".to_string()));
        for (i, col) in cols.iter().enumerate() {
            if i > 0 {
                parts.push(WherePart::Text(" OR ".to_string()));
            }
            parts.push(WherePart::Text(format!("{col} LIKE ")));
            parts.push(WherePart::Bind(like.clone()));
            parts.push(WherePart::Text(" ESCAPE '\\'".to_string()));
        }
        // Tìm được cả theo hóa đơn đại diện: gõ số HĐĐT / số hóa đơn nội bộ
        // vẫn ra phiếu xuất đang trỏ tới hóa đơn đó.
        parts.push(WherePart::Text(
            " OR EXISTS (SELECT 1 FROM invoice i
                          WHERE i.voucher_no = je.voucher_no
                            AND (i.number LIKE "
                .to_string(),
        ));
        parts.push(WherePart::Bind(like.clone()));
        parts.push(WherePart::Text(
            " ESCAPE '\\' OR i.e_invoice_no LIKE ".to_string(),
        ));
        parts.push(WherePart::Bind(like));
        parts.push(WherePart::Text(" ESCAPE '\\'))".to_string()));
        parts.push(WherePart::Text(")".to_string()));
    }
    parts
}

/// Đẩy các mảnh điều kiện vào builder theo đúng thứ tự.
fn push_voucher_where<'q>(qb: &mut sqlx::QueryBuilder<'q, Sqlite>, parts: &[WherePart]) {
    for part in parts {
        match part {
            WherePart::Text(t) => {
                qb.push(t.clone());
            }
            WherePart::Bind(v) => {
                qb.push_bind(v.clone());
            }
        }
    }
}

/// Phần `SELECT ... FROM journal_entry` dùng chung cho truy vấn lấy dòng và
/// truy vấn đếm — phải GIỐNG NHAU, nếu không thì điều kiện lọc theo tên cột đã
/// đặt bí danh (`supplier_name`…) sẽ không tồn tại trong truy vấn đếm.
const VOUCHER_INNER_SQL: &str = "SELECT je.posting_date, je.voucher_no, je.entry_type,
                je.description, je.product_code, je.quantity, je.discount, je.amount,
                je.note,
                COALESCE(s.name, '') AS supplier_name,
                COALESCE(c.name, '') AS customer_name
           FROM journal_entry je
           LEFT JOIN supplier s ON s.code = je.supplier_code
           LEFT JOIN customer c ON c.code = je.customer_code
          WHERE ";

/// Gắn hóa đơn đại diện (nếu có) vào từng dòng phiếu xuất của trang hiện tại.
///
/// Liên kết nằm ở `invoice.voucher_no`; một phiếu đúng ra chỉ có 1 hóa đơn,
/// nhưng dữ liệu cũ có thể có nhiều bản cùng trỏ một phiếu — lấy bản mới nhất
/// (id lớn nhất) để hiển thị ổn định, việc dọn dứt điểm do thao tác "Liên kết
/// lại" đảm nhiệm.
async fn attach_invoice_links(
    pool: &SqlitePool,
    rows: &mut [StockVoucherRow],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT id, number, e_invoice_no, status, voucher_no FROM invoice
          WHERE voucher_no IN (",
    );
    let mut sep = qb.separated(", ");
    for row in rows.iter() {
        sep.push_bind(row.voucher_no.clone());
    }
    sep.push_unseparated(") ORDER BY id DESC");
    let links: Vec<(i64, String, String, String, String)> = qb
        .build_query_as()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    for (id, number, e_invoice_no, status, voucher_no) in links {
        if let Some(row) = rows
            .iter_mut()
            .find(|r| r.voucher_no == voucher_no && r.invoice_id.is_none())
        {
            row.invoice_id = Some(id);
            row.invoice_number = number;
            row.invoice_e_invoice_no = e_invoice_no;
            row.invoice_status = status;
        }
    }
    Ok(())
}

/// Thân truy vấn danh sách phiếu — tách riêng để unit test gọi trực tiếp.
pub(crate) async fn stock_vouchers_core(
    pool: &SqlitePool,
    entry_type: &str,
    from_date: &str,
    to_date: &str,
    search: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<StockVoucherRow>, i64), String> {
    let parts = voucher_where_parts(entry_type, from_date, to_date, search);
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT voucher_no, MIN(posting_date) AS posting_date, entry_type,
                MIN(description) AS description,
                MAX(supplier_name) AS supplier_name, MAX(customer_name) AS customer_name,
                SUM(CASE WHEN product_code <> '' THEN 1 ELSE 0 END) AS line_count,
                COUNT(DISTINCT CASE WHEN product_code <> '' THEN product_code END) AS item_count,
                SUM(quantity) AS total_qty,
                SUM(discount) AS discount,
                SUM(amount) AS amount,
                MAX(note) AS note
           FROM (
             SELECT je.posting_date, je.voucher_no, je.entry_type, je.description,
                    je.product_code, je.quantity, je.discount, je.amount, je.note,
                    COALESCE(s.name, '') AS supplier_name,
                    COALESCE(c.name, '') AS customer_name
               FROM journal_entry je
               LEFT JOIN supplier s ON s.code = je.supplier_code
               LEFT JOIN customer c ON c.code = je.customer_code
             WHERE ",
    );
    push_voucher_where(&mut qb, &parts);
    qb.push(") v GROUP BY voucher_no, entry_type ORDER BY MIN(posting_date) DESC, voucher_no DESC LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);
    let mut rows: Vec<StockVoucherRow> = qb
        .build_query_as::<StockVoucherRow>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    attach_invoice_links(pool, &mut rows).await?;

    // Tổng số phiếu (COUNT DISTINCT theo cùng bộ lọc) để bảng đếm đúng số trang.
    let mut cq = sqlx::QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM (SELECT 1 FROM (");
    cq.push(VOUCHER_INNER_SQL);
    push_voucher_where(&mut cq, &parts);
    cq.push(") v GROUP BY voucher_no, entry_type) g");
    let total: i64 = cq.build_query_scalar().fetch_one(pool).await.unwrap_or(0);

    Ok((rows, total))
}

/// Một lô sản xuất trên danh sách: đầu phiếu nhập loại `production` gộp với
/// các dòng thành phẩm (PN) của chính lô đó.
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct ProductionLotRow {
    pub voucher_no: String,
    pub posting_date: String,
    pub description: String,
    /// Thành phẩm của lô, gộp bằng ", " (thường 1 lô = 1 thành phẩm).
    pub products: String,
    /// Số LOẠI thành phẩm của lô.
    pub item_count: i64,
    pub total_qty: f64,
    /// Giá trị nhập kho = giá thành (Nợ 155 / Có 154), tính bằng đ.
    pub amount: f64,
    pub warehouse_code: String,
}

/// Điều kiện lọc danh sách lô (chỉ đụng cột đầu phiếu + EXISTS dòng hàng nên
/// dùng lại y hệt cho câu đếm).
fn production_lots_where(search: &str, from_date: &str, to_date: &str) -> (String, Vec<String>) {
    let mut sql = String::from(" WHERE iv.inbound_type = 'production'");
    let mut params: Vec<String> = Vec::new();
    if !from_date.trim().is_empty() {
        sql.push_str(" AND iv.posting_date >= ?");
        params.push(from_date.trim().to_string());
    }
    if !to_date.trim().is_empty() {
        sql.push_str(" AND iv.posting_date <= ?");
        params.push(to_date.trim().to_string());
    }
    let kw = search.trim();
    if !kw.is_empty() {
        // Khóa ký tự đại diện để gõ `%` cũng chỉ tìm đúng chữ đó.
        let like = format!(
            "%{}%",
            kw.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        sql.push_str(
            " AND (iv.voucher_no LIKE ? ESCAPE '\\' OR iv.description LIKE ? ESCAPE '\\'
               OR EXISTS (SELECT 1 FROM journal_entry je
                            LEFT JOIN product p ON p.code = je.product_code
                           WHERE je.inbound_voucher_id = iv.id AND je.entry_type = 'PN'
                             AND (je.product_code LIKE ? ESCAPE '\\' OR p.name LIKE ? ESCAPE '\\')))",
        );
        params.extend([like.clone(), like.clone(), like.clone(), like.clone()]);
    }
    (sql, params)
}

/// Thân truy vấn danh sách lô sản xuất — tách riêng để unit test gọi trực tiếp.
pub(crate) async fn production_lots_core(
    pool: &SqlitePool,
    search: &str,
    from_date: &str,
    to_date: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ProductionLotRow>, i64), String> {
    let (where_sql, params) = production_lots_where(search, from_date, to_date);

    // 1) Đầu phiếu + tổng hợp dòng thành phẩm (mỗi lô 1 dòng).
    let rows_sql = format!(
        "SELECT iv.voucher_no, iv.posting_date, iv.description,
                CAST(iv.total AS REAL) AS amount,
                iv.warehouse_code,
                '' AS products,
                COUNT(DISTINCT CASE WHEN je.product_code <> '' THEN je.product_code END)
                    AS item_count,
                CAST(COALESCE(SUM(CASE WHEN je.product_code <> '' THEN je.quantity END), 0) AS REAL)
                    AS total_qty
           FROM inbound_voucher iv
           LEFT JOIN journal_entry je
             ON je.inbound_voucher_id = iv.id AND je.entry_type = 'PN'
          {where_sql}
          GROUP BY iv.voucher_no
          ORDER BY iv.posting_date DESC, iv.voucher_no DESC
          LIMIT ? OFFSET ?"
    );
    let mut q = sqlx::query_as::<_, ProductionLotRow>(&rows_sql);
    for p in &params {
        q = q.bind(p);
    }
    let mut rows: Vec<ProductionLotRow> = q
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    // 2) Tên thành phẩm của đúng các lô đang hiển thị (trang này thôi — không
    //    gộp vào câu trên vì SQLite không gộp được DISTINCT bằng GROUP_CONCAT).
    if !rows.is_empty() {
        let nos: Vec<String> = rows.iter().map(|r| r.voucher_no.clone()).collect();
        let marks = vec!["?"; nos.len()].join(", ");
        let names_sql = format!(
            "SELECT je.voucher_no, COALESCE(NULLIF(p.name, ''), je.product_code)
               FROM journal_entry je
               LEFT JOIN product p ON p.code = je.product_code
              WHERE je.entry_type = 'PN' AND je.product_code <> ''
                AND je.voucher_no IN ({marks})
              ORDER BY je.voucher_no, je.id"
        );
        let mut nq = sqlx::query_as::<_, (String, String)>(&names_sql);
        for no in &nos {
            nq = nq.bind(no);
        }
        let names = nq.fetch_all(pool).await.map_err(|e| e.to_string())?;
        for (no, name) in names {
            if let Some(row) = rows.iter_mut().find(|r| r.voucher_no == no) {
                if row.products.is_empty() {
                    row.products = name;
                } else if !row.products.contains(&name) {
                    row.products.push_str(", ");
                    row.products.push_str(&name);
                }
            }
        }
    }

    // 3) Tổng số lô (cùng bộ lọc) để bảng đếm đúng số trang.
    let count_sql = format!("SELECT COUNT(*) FROM inbound_voucher iv{where_sql}");
    let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
    for p in &params {
        cq = cq.bind(p);
    }
    let total = cq.fetch_one(pool).await.unwrap_or(0);

    Ok((rows, total))
}

/// Danh sách lô sản xuất (phiếu nhập loại `production`) — phân trang server-side.
#[tauri::command]
pub(crate) async fn get_production_lots(
    state: State<'_, AppState>,
    lazy_event: String,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    let pool = state.pool.read().await;
    let (rows, total) = production_lots_core(
        &pool,
        &ev.global_keyword().unwrap_or_default(),
        &from_date,
        &to_date,
        page.size,
        page.offset,
    )
    .await?;
    Ok(json!({ "rows": rows, "total": total }).to_string())
}

/// Một thành phần đã chụp của lô sản xuất (snapshot định mức tại lúc lưu).
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct ProductionLotComponentRow {
    pub product_code: String,
    pub product_name: String,
    pub material_code: String,
    pub material_name: String,
    pub material_unit: String,
    pub is_service: bool,
    pub per_unit_qty: f64,
    pub finished_qty: f64,
    pub quantity: f64,
    pub unit_price: f64,
    pub amount: f64,
    pub unit_cost: f64,
    pub cost_amount: f64,
    pub warehouse_code: String,
}

pub(crate) async fn production_lot_components_core(
    pool: &SqlitePool,
    voucher_no: &str,
) -> Result<Vec<ProductionLotComponentRow>, String> {
    sqlx::query_as!(
        ProductionLotComponentRow,
        r#"SELECT c.product_code as "product_code!", c.product_name as "product_name!",
                  c.material_code as "material_code!", c.material_name as "material_name!",
                  c.material_unit as "material_unit!",
                  c.is_service as "is_service!: bool",
                  c.per_unit_qty as "per_unit_qty!", c.finished_qty as "finished_qty!",
                  c.quantity as "quantity!", c.unit_price as "unit_price!",
                  c.amount as "amount!", c.unit_cost as "unit_cost!",
                  c.cost_amount as "cost_amount!", c.warehouse_code as "warehouse_code!"
             FROM production_lot_component c
             JOIN inbound_voucher iv ON iv.id = c.inbound_voucher_id
            WHERE iv.voucher_no = ?
            ORDER BY c.product_code, c.is_service, c.material_code"#,
        voucher_no
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Bảng thành phần của 1 lô sản xuất — snapshot tại thời điểm tạo lô, không
/// đọc lại định mức hiện hành (định mức có thể đã bị sửa).
#[tauri::command]
pub(crate) async fn get_production_lot_components(
    state: State<'_, AppState>,
    voucher_no: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if voucher_no.trim().is_empty() {
        return Err("Thiếu số phiếu lô".into());
    }
    let rows = {
        let pool = state.pool.read().await;
        production_lot_components_core(&pool, voucher_no.trim()).await?
    };
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
                je.quantity, je.unit_price, je.amount, je.discount,
                je.debit_account, je.credit_account,
                je.vat_rate, je.industry_code, je.note
         FROM journal_entry je
         LEFT JOIN product p ON p.code = je.product_code
         LEFT JOIN supplier s ON s.code = je.supplier_code
         LEFT JOIN customer c ON c.code = je.customer_code
         WHERE je.voucher_no = ?
         -- Dòng giá vốn Nợ 632 / Có 152 sinh kèm khi xuất bán là bút toán KẾ TOÁN,
         -- không phải dòng hàng trên phiếu: cộng vào thì Cộng thành tiền của phiếu
         -- xuất bị thổi lên bằng giá vốn. Nó vẫn hiện ở sổ cái và bảng cân đối.
         AND je.entry_type != 'CP'
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
            discount: 0.0,
            industry_code: "PPHH".into(),
            warehouse_code: "".into(), // rỗng → kho mặc định / kho đầu tiên
            line_name: String::new(),
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
            discount: 0.0,
            industry_code: "PPHH".into(),
            warehouse_code: warehouse_code.into(),
            line_name: String::new(),
        }
    }

    fn inp(product_code: &str, quantity: f64, unit_price: f64) -> InboundItemInput {
        InboundItemInput {
            product_code: product_code.into(),
            quantity,
            unit_price,
            discount: 0.0,
            ..Default::default()
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

    // "Giá tiền tự nhớ": đơn giá nhập vừa giao làm GIÁ VỐN HIỆN TẠI của mặt
    // hàng — lần nhập sau lấy làm mặc định; nhập kho KHÔNG đụng giá bán.
    #[tokio::test]
    async fn nhap_kho_nho_lai_gia_von_hien_tai_khong_dung_gia_ban() {
        let pool = test_pool().await;
        seed_product(&pool, "PN-GIA", "Hàng nhập nhớ giá", 0.01).await;
        sqlx::query!(
            "UPDATE product SET sale_price = 12345.0, cost_price = 111.0 WHERE code = 'PN-GIA'"
        )
        .execute(&pool)
        .await
        .unwrap();

        let items = [InboundItemInput {
            product_code: "PN-GIA".into(),
            quantity: 2.0,
            unit_price: 9_999.0,
            discount: 0.0,
            ..Default::default()
        }];
        save_inbound_core(
            &pool,
            "2026-02-10",
            "PN-GIA1",
            "Nhập nhớ giá",
            "",
            "", // kho rỗng → kho mặc định KHO-CHINH (migration seed)
            "HKD",
            &items,
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
        )
        .await
        .expect("nhập kho");

        let row = sqlx::query!("SELECT sale_price, cost_price FROM product WHERE code = 'PN-GIA'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.cost_price, 9_999.0, "giá nhập làm giá vốn hiện tại");
        assert_eq!(row.sale_price, 12_345.0, "nhập kho không đụng giá bán");
    }

    // Xuất bán LẬP KÈM hóa đơn → giá bán trên phiếu cũng làm giá hiện tại.
    #[tokio::test]
    async fn xuat_ban_lap_kem_hoa_don_nho_lai_gia_ban() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "PX-GIA", "Hàng bán nhớ giá", 0.01).await;
        // Kho mặc định KHO-CHINH do migration seed sẵn.
        let w: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM warehouse WHERE code = 'KHO-CHINH'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-GIA1",
            "Bán nhớ giá",
            "",
            "HKD",
            &[out("PX-GIA", 1.0, 8888.0)],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput {
                number: "HD-XP1".into(),
                ..Default::default()
            },
        )
        .await
        .expect("xuất bán có hóa đơn kèm");
        assert!(!r.invoice_no.is_empty(), "phải lập hóa đơn kèm phiếu");

        let sale = sqlx::query!("SELECT sale_price FROM product WHERE code = 'PX-GIA'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .sale_price;
        assert_eq!(sale, 8888.0, "giá bán trên phiếu xuất làm giá hiện tại");
    }

    #[tokio::test]
    async fn xuat_kho_fifo_va_ghi_so_ban_hang() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        // Kho mặc định được migration seed sẵn (KHO-CHINH) — khi dòng bỏ trống kho,
        // xuất rơi vào kho đầu tiên này (fallback của resolve_warehouse).
        let w: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM warehouse WHERE code = 'KHO-CHINH'"#)
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
        let first_lot = sqlx::query!(
            r#"SELECT quantity, depleted as "depleted: bool" FROM stock_lot
                 ORDER BY received_at ASC LIMIT 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let last_lot = sqlx::query!(
            r#"SELECT quantity, depleted as "depleted: bool" FROM stock_lot
                 ORDER BY received_at DESC LIMIT 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((first_lot.quantity, first_lot.depleted), (0.0, true));
        assert_eq!((last_lot.quantity, last_lot.depleted), (5.0, false));

        // Sổ bán hàng: Nợ 131 / Có 511, đúng tỷ lệ ngành PPHH
        let je = sqlx::query!(
            "SELECT entry_type, debit_account, credit_account, amount, vat_rate
             FROM journal_entry WHERE voucher_no = 'PXK-01'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(je.entry_type, "PX");
        assert_eq!(je.debit_account, "131");
        assert_eq!(je.credit_account, "511");
        assert_eq!(je.amount, 30_000.0);
        assert_eq!(je.vat_rate, 0.01);
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
        let lot = sqlx::query!(
            r#"SELECT quantity, warehouse_id as "wh!: i64" FROM stock_lot WHERE warehouse_id = ?"#,
            w1
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(lot.quantity, 10.0);
        assert_eq!(lot.wh, w1);

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
        sqlx::query!(
            "UPDATE product SET is_service = 1, industry_code = 'DVXD-KNL' WHERE code = 'S1'"
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
                discount: 0.0,
                industry_code: "".into(), // rỗng → lấy nhóm ngành của sản phẩm
                warehouse_code: "".into(),

                line_name: String::new(),
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

        let je = sqlx::query!(
            "SELECT entry_type, debit_account, credit_account, amount, industry_code, vat_rate, pit_rate
             FROM journal_entry WHERE voucher_no = 'PXK-SV'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(je.entry_type, "PX");
        assert_eq!(je.debit_account, "131");
        assert_eq!(je.credit_account, "511");
        assert_eq!(je.amount, 1_500_000.0);
        assert_eq!(je.industry_code, "DVXD-KNL");
        assert_eq!(je.vat_rate, 0.05);
        assert_eq!(je.pit_rate, 0.02);
    }

    // ─── Xuất bán phải ghi giá vốn: cặp Nợ 632 / Có 152 ───

    #[tokio::test]
    async fn xuat_ban_ghi_gia_von_632_co_152() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "P1", "Hàng A", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;

        // Bán 5 x 2.000, giá vốn FIFO 5 x 1.000 = 5.000
        let r = save_outbound_core(
            &pool,
            "2026-05-20",
            "PXK-GV",
            "Bán hàng ghi giá vốn",
            "KH1",
            "HKD",
            &[out_wh("P1", 5.0, 2000.0, "W1")],
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho bán hàng");
        assert_eq!(r.revenue, 10_000.0);
        assert_eq!(r.cogs, 5_000.0);

        // Hai bút toán: doanh thu (131/511) và giá vốn (632/152).
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = 'PXK-GV'"
            )
            .await,
            2
        );
        let je = sqlx::query!(
            "SELECT entry_type, debit_account, credit_account, amount, quantity, unit_price
             FROM journal_entry WHERE voucher_no = 'PXK-GV' AND debit_account = '632'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(je.entry_type, "CP");
        assert_eq!(je.credit_account, "152");
        assert_eq!(je.amount, 5_000.0);
        assert_eq!(je.quantity, 5.0);
        assert_eq!(je.unit_price, 1000.0);
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
        let je = sqlx::query!(
            "SELECT entry_type, debit_account, credit_account, amount, customer_code, note
             FROM journal_entry WHERE voucher_no = 'PT001'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(je.entry_type, "PT");
        assert_eq!(je.debit_account, "111");
        assert_eq!(je.credit_account, "131");
        assert_eq!(je.amount, 10_000.0);
        assert_eq!(je.customer_code, "KH1");
        // Liên kết: ghi chú chứa số PX để truy vết PT ↔ PX
        assert!(je.note.contains("PXK-03"), "ghi chú: {}", je.note);

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
    async fn chiet_khau_duoc_luu_va_gom_phieu_mot_dong() {
        // Một phiếu 3 dòng hàng (2 loại) có chiết khấu → màn kho phải ra MỘT dòng
        // với số loại mặt hàng = 2, chiết khấu = tổng Tiền CK, thành tiền = sau CK.
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_product(&pool, "P2", "Hàng B", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        let items = vec![
            InboundItemInput {
                product_code: "P1".into(),
                quantity: 2.0,
                unit_price: 1_000_000.0,
                discount: 200_000.0,
                ..Default::default()
            },
            InboundItemInput {
                product_code: "P1".into(), // cùng loại, dòng thứ 2
                quantity: 1.0,
                unit_price: 1_000_000.0,
                discount: 100_000.0,
                ..Default::default()
            },
            InboundItemInput {
                product_code: "P2".into(),
                quantity: 1.0,
                unit_price: 500_000.0,
                discount: 0.0,
                ..Default::default()
            },
        ];
        save_inbound_core(
            &pool,
            "2026-02-10",
            "PN-CK",
            "Nhập có chiết khấu",
            "",
            "W1",
            "HKD",
            &items,
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
        )
        .await
        .expect("nhập kho có chiết khấu");

        // Tiền CK được ghi lại trên từng dòng (không chỉ trừ vào giá trị).
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT COALESCE(SUM(discount), 0) FROM journal_entry WHERE voucher_no = 'PN-CK'"
            )
            .await,
            300_000.0
        );

        let (rows, total) = stock_vouchers_core(&pool, "PN", "", "", "", 20, 0)
            .await
            .expect("danh sách phiếu");
        assert_eq!(total, 1);
        assert_eq!(rows.len(), 1, "1 phiếu = 1 dòng");
        let v = &rows[0];
        assert_eq!(v.voucher_no, "PN-CK");
        assert_eq!(v.item_count, 2, "số LOẠI mặt hàng, không phải số lượng");
        assert_eq!(v.total_qty, 4.0);
        assert_eq!(v.discount, 300_000.0);
        // Thành tiền = 2tr + 1tr + 500k − 300k CK = 3,2 triệu.
        assert_eq!(v.amount, 3_200_000.0);

        // Ô tìm kiếm: lọc theo số phiếu / diễn giải, không vỡ cú pháp SQL.
        let descs: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT description FROM journal_entry WHERE voucher_no = 'PN-CK'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        eprintln!("DEBUG6 descs={:?}", descs);
        let (found, found_total) = stock_vouchers_core(&pool, "PN", "", "", "chiết khấu", 20, 0)
            .await
            .expect("tìm theo diễn giải");
        assert_eq!(found_total, 1);
        assert_eq!(found[0].voucher_no, "PN-CK");
        let (none, none_total) = stock_vouchers_core(&pool, "PN", "", "", "khong-ton-tai", 20, 0)
            .await
            .expect("tìm từ khoá không khớp");
        assert_eq!(none_total, 0);
        assert!(none.is_empty());
    }

    /// Màn phiếu xuất ánh xạ ngược về HĐĐT qua `invoice.voucher_no`: dòng phiếu
    /// có sẵn số hóa đơn/HĐĐT đại diện, và gõ số HĐĐT cũng tìm ra phiếu.
    #[tokio::test]
    async fn danh_sach_phieu_xuat_hien_hoa_don_dai_dien_va_tim_theo_so_hddt() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "PX-LINK", "Hàng liên kết", 0.01).await;
        let w = seed_warehouse(&pool, "W-LINK", "Kho liên kết").await;
        add_stock_lot(&pool, p, w, 10.0, 1_000.0, "2026-01-01").await;
        save_outbound_core(
            &pool,
            "2026-03-10",
            "PX-LINK1",
            "Bán hàng theo hóa đơn HD-LINK1",
            "",
            "HKD",
            &[OutboundItemInput {
                product_code: "PX-LINK".into(),
                quantity: 1.0,
                unit_price: 5_000.0,
                discount: 0.0,
                industry_code: "PPHH".into(),
                warehouse_code: "W-LINK".into(),
                line_name: String::new(),
            }],
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput {
                number: "HD-LINK1".into(),
                e_invoice_no: "0000123".into(),
                ..Default::default()
            },
        )
        .await
        .expect("lập phiếu xuất kèm hóa đơn");

        let (rows, total) = stock_vouchers_core(&pool, "PX", "", "", "", 20, 0)
            .await
            .expect("danh sách phiếu xuất");
        assert_eq!(total, 1);
        let row = &rows[0];
        assert!(row.invoice_id.is_some(), "phải ánh xạ được hóa đơn");
        assert_eq!(row.invoice_number, "HD-LINK1");
        assert_eq!(row.invoice_e_invoice_no, "0000123");
        assert_eq!(row.invoice_status, "official");

        let (by_hddt, _) = stock_vouchers_core(&pool, "PX", "", "", "0000123", 20, 0)
            .await
            .expect("tìm theo số HĐĐT");
        assert_eq!(by_hddt.len(), 1);
        assert_eq!(by_hddt[0].voucher_no, "PX-LINK1");
        let (by_number, _) = stock_vouchers_core(&pool, "PX", "", "", "HD-LINK1", 20, 0)
            .await
            .expect("tìm theo số hóa đơn nội bộ");
        assert_eq!(by_number.len(), 1);
        assert_eq!(by_number[0].voucher_no, "PX-LINK1");
    }

    /// Danh sách lô sản xuất: chỉ lấy đầu phiếu loại `production`, mỗi lô 1 dòng
    /// kèm tên thành phẩm — phiếu nhập mua ngoài cùng thời điểm không bị lẫn.
    #[tokio::test]
    async fn danh_sach_lo_san_xuat_chi_lay_phieu_loai_production() {
        let pool = test_pool().await;
        seed_product(&pool, "FG1", "Thành phẩm A", 0.01).await;
        seed_product(&pool, "FG2", "Thành phẩm B", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;

        save_inbound_core(
            &pool,
            "2026-03-02",
            "PN-LSX1",
            "Lô sản xuất A",
            "",
            "W1",
            "HKD",
            &[inp("FG1", 10.0, 5_000.0)],
            "",
            "production",
            "",
            0.0,
            "155",
            "154",
            false,
            "",
            false,
        )
        .await
        .expect("lô sản xuất");
        save_inbound_core(
            &pool,
            "2026-03-01",
            "PN-MUA1",
            "Nhập mua ngoài",
            "",
            "W1",
            "HKD",
            &[inp("FG2", 3.0, 1_000.0)],
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
        )
        .await
        .expect("nhập mua ngoài");

        let (rows, total) = production_lots_core(&pool, "", "", "", 20, 0)
            .await
            .expect("danh sách lô");
        assert_eq!(total, 1, "phiếu mua ngoài không phải lô sản xuất");
        let lot = &rows[0];
        assert_eq!(lot.voucher_no, "PN-LSX1");
        assert_eq!(
            lot.products, "Thành phẩm A",
            "tên thành phẩm ghép từ dòng PN"
        );
        assert_eq!(lot.item_count, 1);
        assert_eq!(lot.total_qty, 10.0);
        assert_eq!(lot.amount, 50_000.0, "giá trị nhập = giá thành");
        assert_eq!(lot.warehouse_code, "W1");

        // Từ khoá tìm qua TÊN thành phẩm của lô.
        let (found, found_total) = production_lots_core(&pool, "Thành phẩm A", "", "", 20, 0)
            .await
            .expect("tìm theo tên thành phẩm");
        assert_eq!(found_total, 1);
        assert_eq!(found[0].voucher_no, "PN-LSX1");
        let (_, other_total) = production_lots_core(&pool, "Thành phẩm B", "", "", 20, 0)
            .await
            .expect("tìm theo tên của phiếu mua ngoài");
        assert_eq!(other_total, 0, "không lẫn phiếu mua ngoài");

        // Khoảng ngày.
        let (_, future_total) = production_lots_core(&pool, "", "2026-03-10", "", 20, 0)
            .await
            .expect("lọc từ ngày");
        assert_eq!(future_total, 0);
    }

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
            false,
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
        set_vat_deduct(&pool, true).await; // hộ nộp thuế theo lợi nhuận

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
            false,
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

    /// Hộ KHÔNG khấu trừ GTGT (mặc định): thuế đầu vào phải nằm trong giá vốn
    /// và giá trị nhập kho, không sinh bút toán 133.
    #[tokio::test]
    async fn nhap_kho_khong_khau_tru_cong_thue_vao_gia_von() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng A", 0.01).await;
        seed_warehouse(&pool, "W1", "Kho 1").await;
        // test_pool seed sẵn vat_deduct = '0' (hộ nộp thuế theo doanh thu).

        // Mua 10 × 10.000, VAT 10% → Nợ 152 / Có 331: 110.000 (gồm thuế).
        let r = save_inbound_core(
            &pool,
            "2026-02-11",
            "PNK-KT01",
            "Mua hàng theo HĐ 124",
            "NCC1",
            "W1",
            "HKD",
            &[inp("P1", 10.0, 10_000.0)],
            "",
            "purchase",
            "HĐ 124",
            0.1,
            "152",
            "331",
            false,
            "",
            false,
        )
        .await
        .expect("nhập kho không khấu trừ");

        assert_eq!(r.total, 110_000.0, "giá trị nhập kho phải gồm thuế");
        assert_eq!(r.vat, 0.0, "không khấu trừ thì không tách VAT");

        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = 'PNK-KT01'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT COALESCE(SUM(amount),0) FROM journal_entry
                 WHERE voucher_no = 'PNK-KT01' AND debit_account = '152' AND credit_account = '331'"
            )
            .await,
            110_000.0
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry
                 WHERE voucher_no = 'PNK-KT01' AND debit_account = '133'"
            )
            .await,
            0
        );
        // Giá vốn FIFO + giá vốn mặc định của mặt hàng đều đã gồm thuế.
        assert_eq!(
            scalar_f64(&pool, "SELECT unit_cost FROM stock_lot").await,
            11_000.0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT cost_price FROM product WHERE code = 'P1'").await,
            11_000.0
        );
        // Đầu phiếu: tổng gồm thuế, vat_amount = 0 (không được khấu trừ).
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT total FROM inbound_voucher WHERE voucher_no = 'PNK-KT01'"
            )
            .await,
            110_000.0
        );
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT vat_amount FROM inbound_voucher WHERE voucher_no = 'PNK-KT01'"
            )
            .await,
            0.0
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
            false,
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
                ..Default::default()
            }],
            "",
            "purchase",
            "HĐ 88",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
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
        set_vat_deduct(&pool, true).await; // hộ nộp thuế theo lợi nhuận

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
            false,
        )
        .await
        .expect("nhập kho trả tiền ngay");

        assert_eq!(r.total, 100_000.0);
        assert_eq!(r.vat, 10_000.0);

        // Phiếu chi: Nợ 331 (phải trả NCC) / Có 111 (tiền mặt), đủ 110.000 = hàng + thuế
        let je = sqlx::query!(
            "SELECT entry_type, debit_account, credit_account, amount, vat_rate,
                    supplier_code, note
             FROM journal_entry WHERE voucher_no = 'PC001'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(je.entry_type, "PC");
        assert_eq!(je.debit_account, "331");
        assert_eq!(je.credit_account, "111");
        assert_eq!(je.amount, 110_000.0);
        assert_eq!(je.vat_rate, 0.0);
        // Liên kết: ghi chú chứa số PNK + gắn đúng nhà cung cấp
        assert_eq!(je.supplier_code, "NCC1");
        assert!(je.note.contains("PNK-05"), "ghi chú: {}", je.note);

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
            false,
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
            false,
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
            false,
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
        set_vat_deduct(&pool, true).await; // hộ nộp thuế theo lợi nhuận

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
            false,
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
        let q1: f64 =
            sqlx::query_scalar!("SELECT quantity FROM stock_lot ORDER BY received_at ASC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(q1, 5.0); // lô đầu trừ 5

        // Sổ đảo ngược: Nợ 331 / Có 152 (hàng) + Nợ 331 / Có 133 (thuế), adjust = GiamCP
        let je = sqlx::query!(
            "SELECT debit_account, credit_account, adjust_code FROM journal_entry
             WHERE voucher_no = 'PNK-DC01' AND debit_account = '331' AND credit_account = '152'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (
                je.debit_account.as_str(),
                je.credit_account.as_str(),
                je.adjust_code.as_str()
            ),
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
            false,
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
            false,
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
        let je = sqlx::query!(
            "SELECT debit_account, credit_account FROM journal_entry WHERE voucher_no = 'PNK-DC03'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (je.debit_account.as_str(), je.credit_account.as_str()),
            ("152", "331")
        );
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
        let cost: f64 =
            sqlx::query_scalar!("SELECT unit_cost FROM stock_lot ORDER BY id DESC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cost, 2000.0);

        // PX: Nợ 511 / Có 131 (giảm doanh thu), adjust = GiamDT
        let px = sqlx::query!(
            "SELECT debit_account, credit_account, adjust_code FROM journal_entry
             WHERE voucher_no = 'PXK-DC01' AND entry_type = 'PX'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (
                px.debit_account.as_str(),
                px.credit_account.as_str(),
                px.adjust_code.as_str()
            ),
            ("511", "131", "GiamDT")
        );
        // PN: Nợ 152 / Có 632 (nhập lại hàng, giảm giá vốn)
        let pn = sqlx::query!(
            "SELECT debit_account, credit_account FROM journal_entry
             WHERE voucher_no = 'PXK-DC01' AND entry_type = 'PN'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (pn.debit_account.as_str(), pn.credit_account.as_str()),
            ("152", "632")
        );
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
    /// Tìm phiếu theo số / mã hàng / khách và lọc khoảng ngày — danh sách sắp
    /// theo ngày nên phiếu cũ nằm giữa, phải có đường tìm nhanh.
    /// Số phiếu nhập / xuất trùng thì chặn — trùng số là mất khả năng tra cứu
    /// chứng từ (và với phiếu nhập còn khiến HĐĐT neo nhầm vào phiếu cũ).
    #[tokio::test]
    async fn so_phieu_nhap_xuat_trung_thi_bao_loi() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "SP-VN", "Hàng", 0.01).await;
        let w = seed_warehouse(&pool, "W-VN", "Kho số phiếu").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;
        let items_in = [InboundItemInput {
            product_code: "SP-VN".into(),
            quantity: 2.0,
            unit_price: 1000.0,
            discount: 0.0,
            ..Default::default()
        }];
        let items_out = [OutboundItemInput {
            product_code: "SP-VN".into(),
            quantity: 1.0,
            unit_price: 1000.0,
            discount: 0.0,
            industry_code: "PPHH".into(),
            warehouse_code: "W-VN".into(),

            line_name: String::new(),
        }];

        save_inbound_core(
            &pool,
            "2026-09-01",
            "PN9100",
            "Phiếu nhập",
            "",
            "W-VN",
            "HKD",
            &items_in,
            "",
            "purchase",
            "",
            0.0,
            "",
            "",
            false,
            "up",
            false,
        )
        .await
        .unwrap();
        let err = save_inbound_core(
            &pool,
            "2026-09-02",
            "PN9100",
            "Phiếu nhập lần 2",
            "",
            "W-VN",
            "HKD",
            &items_in,
            "",
            "purchase",
            "",
            0.0,
            "",
            "",
            false,
            "up",
            false,
        )
        .await
        .unwrap_err();
        assert!(err.contains("Số phiếu nhập PN9100 đã tồn tại"), "{err}");
        // Khác hoa/thường cũng là trùng số.
        let err = save_inbound_core(
            &pool,
            "2026-09-02",
            "pn9100",
            "Phiếu nhập lần 3",
            "",
            "W-VN",
            "HKD",
            &items_in,
            "",
            "purchase",
            "",
            0.0,
            "",
            "",
            false,
            "up",
            false,
        )
        .await
        .unwrap_err();
        assert!(err.contains("đã tồn tại"), "{err}");

        save_outbound_core(
            &pool,
            "2026-09-03",
            "PX9100",
            "Phiếu xuất",
            "",
            "HKD",
            &items_out,
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap();
        let err = save_outbound_core(
            &pool,
            "2026-09-04",
            "PX9100",
            "Phiếu xuất lần 2",
            "",
            "HKD",
            &items_out,
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Số phiếu xuất PX9100 đã tồn tại"), "{err}");

        // Phiếu xuất lưu 2 dòng bút toán — lần lưu trùng phải rollback trọn vẹn,
        // tồn kho không bị trừ thêm lần nữa.
        let qty: f64 = sqlx::query_scalar!(
            "SELECT quantity FROM stock_lot WHERE product_id = ? AND depleted = 0",
            p
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(qty, 19.0);
        let n: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) as \"n!: i64\" FROM journal_entry WHERE entry_type = 'PX' AND voucher_no = 'PX9100'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "chỉ có 1 dòng Nợ 131 của phiếu PX9100");
    }

    #[tokio::test]
    async fn tim_phieu_theo_tu_khoa_va_loc_ngay() {
        let pool = test_pool().await;
        let p1 = seed_product(&pool, "SP-T1", "Bột nở", 0.01).await;
        let p2 = seed_product(&pool, "SP-T2", "Dầu ăn", 0.01).await;
        let _w = seed_warehouse(&pool, "W-T1", "Kho tìm").await;
        add_stock_lot(&pool, p1, _w, 100.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p2, _w, 100.0, 2000.0, "2026-01-01").await;
        seed_customer(&pool, "KH-T1", "Chị Hồng").await;

        let px = OutboundItemInput {
            product_code: "SP-T1".into(),
            quantity: 2.0,
            unit_price: 1000.0,
            discount: 0.0,
            industry_code: "PPHH".into(),
            warehouse_code: "W-T1".into(),

            line_name: String::new(),
        };
        let px2 = OutboundItemInput {
            product_code: "SP-T2".into(),
            quantity: 2.0,
            unit_price: 2000.0,
            discount: 0.0,
            industry_code: "PPHH".into(),
            warehouse_code: "W-T1".into(),

            line_name: String::new(),
        };

        let old = save_outbound_core(
            &pool,
            "2026-09-01",
            "PX9001",
            "Phiếu cũ",
            "",
            "HKD",
            &[px],
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap();
        let new = save_outbound_core(
            &pool,
            "2026-09-20",
            "PX9002",
            "Phiếu mới",
            "KH-T1",
            "HKD",
            &[px2],
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput::default(),
        )
        .await
        .unwrap();
        assert_eq!(old.entries, 1);
        assert_eq!(new.entries, 1);

        // Không lọc: cả 2 phiếu, mới nhất xếp trước.
        let all = get_journal_entries_core(&pool, "PX", "", "", "")
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].voucher_no, "PX9002");

        // Từ khoá theo số phiếu.
        let by_no = get_journal_entries_core(&pool, "PX", "", "", "px9001")
            .await
            .unwrap();
        assert_eq!(by_no.len(), 1);
        assert_eq!(by_no[0].voucher_no, "PX9001");

        // Từ khoá theo tên hàng (tiếng Việt có dấu) và theo tên khách.
        let by_item = get_journal_entries_core(&pool, "PX", "", "", "dầu ăn")
            .await
            .unwrap();
        assert_eq!(by_item.len(), 1);
        assert_eq!(by_item[0].voucher_no, "PX9002");
        let by_cust = get_journal_entries_core(&pool, "PX", "", "", "hồng")
            .await
            .unwrap();
        assert_eq!(by_cust.len(), 1);
        assert_eq!(by_cust[0].voucher_no, "PX9002");

        // Lọc khoảng ngày: chỉ lấy phiếu trong khoảng.
        let range = get_journal_entries_core(&pool, "PX", "2026-09-10", "2026-09-30", "")
            .await
            .unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].voucher_no, "PX9002");
        let none = get_journal_entries_core(&pool, "PX", "2026-10-01", "2026-10-31", "")
            .await
            .unwrap();
        assert!(none.is_empty());

        // Từ khoá không khớp gì → rỗng (không trả về toàn bộ danh sách).
        let miss = get_journal_entries_core(&pool, "PX", "", "", "khong-ton-tai")
            .await
            .unwrap();
        assert!(miss.is_empty());
        // Lọc nhập kho không lẫn phiếu xuất.
        let pn = get_journal_entries_core(&pool, "PN", "", "", "PX9002")
            .await
            .unwrap();
        assert!(pn.is_empty());
    }

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

        let inv = sqlx::query!(
            "SELECT number, status, customer, customer_tax_code, total, voucher_no
             FROM invoice"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (inv.number.as_str(), inv.status.as_str()),
            ("PXK-INV1", "draft")
        );
        assert_eq!(inv.customer, "Cửa hàng Mây");
        assert_eq!(inv.customer_tax_code, "MST-KH1");
        assert_eq!(inv.total, 6_000.0); // 3 x 2000
        assert_eq!(inv.voucher_no, "PXK-INV1");

        // Dòng chi tiết hóa đơn: đủ hàng, nhóm ngành + tỷ lệ thuế theo bút toán.
        let item =
            sqlx::query!("SELECT quantity, unit_price, subtotal, industry_code FROM invoice_item")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            (item.quantity, item.unit_price, item.subtotal),
            (3.0, 2000.0, 6_000.0)
        );
        assert_eq!(item.industry_code, "PPHH");
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
        let inv = sqlx::query!(
            "SELECT status, e_invoice_no, e_invoice_symbol, e_invoice_date FROM invoice"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.status, "official");
        assert_eq!(inv.e_invoice_no, "E2026/001");
        assert_eq!(inv.e_invoice_symbol, "1C26TT152");
        assert_eq!(inv.e_invoice_date, "2026-07-02");
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

    // ─── F4 — TỰ XUẤT NVL THEO ĐỊNH MỨC KHI LƯU PNK SẢN XUẤT ───

    /// Dựng định mức 1 TP = 2 NVL + tồn NVL trong kho.
    async fn seed_bom(pool: &SqlitePool, stock: f64) -> (i64, i64) {
        let tp = seed_product(pool, "TP1", "Thành phẩm A", 0.01).await;
        let nvl = seed_product(pool, "NVL1", "Vật tư B", 0.01).await;
        let w = seed_warehouse(pool, "W1", "Kho 1").await;
        sqlx::query!(
            "INSERT INTO product_bom_item (product_id, material_product_id, quantity)
             VALUES (?, ?, 2.0)",
            tp,
            nvl
        )
        .execute(pool)
        .await
        .unwrap();
        add_stock_lot(pool, nvl, w, stock, 5_000.0, "2026-01-01").await;
        (tp, nvl)
    }

    #[tokio::test]
    async fn pnk_san_xuat_tu_sinh_phieu_xuat_nvl_theo_dinh_muc() {
        let pool = test_pool().await;
        let (tp, _nvl) = seed_bom(&pool, 100.0).await;

        // Dịch vụ trong định mức (nhân công) không có tồn kho: vẫn lưu được
        // nhưng khi tự xuất NVL phải bỏ qua, không làm hỏng phiếu nhập.
        let svc = seed_product(&pool, "DV1", "Nhân công", 0.0).await;
        sqlx::query!("UPDATE product SET is_service = 1 WHERE id = ?", svc)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO product_bom_item (product_id, material_product_id, quantity)
             VALUES (?, ?, 3.0)",
            tp,
            svc
        )
        .execute(&pool)
        .await
        .unwrap();

        let r = save_inbound_core(
            &pool,
            "2026-03-15",
            "PNK-BOM",
            "Nhập thành phẩm lệnh 01",
            "",
            "W1",
            "HKD",
            &[inp("TP1", 10.0, 8_000.0)],
            "",
            "production",
            "",
            0.0,
            "155",
            "154",
            false,
            "",
            true,
        )
        .await
        .expect("nhập sản xuất tự xuất NVL theo định mức");

        // Số phiếu PX tự sinh trả về cho người dùng biết để tra sổ.
        assert_eq!(r.material_voucher, "PX001");
        assert_eq!(r.total, 80_000.0);

        // Bút toán tự sinh: Nợ TK chi phí của PNK (154) / Có 152 hàng hóa.
        let px = sqlx::query!(
            r#"SELECT product_code, amount, quantity, debit_account, credit_account,
                      adjust_code, industry_code, note, description
                 FROM journal_entry
                WHERE voucher_no = 'PX001' AND entry_type = 'PX'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(px.product_code, "NVL1");
        assert_eq!(px.debit_account, "154");
        assert_eq!(px.credit_account, "152");
        assert_eq!(px.adjust_code, "XuatNVL");
        assert_eq!(px.industry_code, "");
        assert_eq!(px.quantity, 20.0); // 2 NVL x 10 thành phẩm
        assert_eq!(px.amount, 100_000.0); // 20 x 5.000 (giá vốn FIFO)
        assert_eq!(px.note, "Theo PNK-BOM");
        assert!(px.description.contains("PNK-BOM"), "{}", px.description);
        // Chỉ vật tư thật sinh bút toán xuất kho; dòng dịch vụ bị bỏ qua.
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PX'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PX' AND product_code = 'DV1'"
            )
            .await,
            0
        );

        // Tồn NVL: 100 → 80.
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT COALESCE(SUM(sl.quantity), 0) FROM stock_lot sl
                   JOIN product p ON p.id = sl.product_id WHERE p.code = 'NVL1'"
            )
            .await,
            80.0
        );

        // Phiếu tự sinh KHÔNG phải doanh thu → sổ doanh thu kỳ này = 0.
        let v = crate::commands::accounting::get_revenue_expense_core(
            &pool,
            "2026-03-01",
            "2026-03-31",
        )
        .await
        .unwrap();
        assert_eq!(v["revenue_up"].as_f64().unwrap(), 0.0);

        // Snapshot thành phần: chụp cả NVL lẫn dịch vụ, kèm giá vốn FIFO thật.
        let comps = production_lot_components_core(&pool, "PNK-BOM")
            .await
            .unwrap();
        assert_eq!(comps.len(), 2);
        let nvl = comps.iter().find(|c| c.material_code == "NVL1").unwrap();
        assert!(!nvl.is_service);
        assert_eq!(nvl.per_unit_qty, 2.0);
        assert_eq!(nvl.finished_qty, 10.0);
        assert_eq!(nvl.quantity, 20.0);
        assert_eq!(nvl.unit_cost, 5_000.0);
        assert_eq!(nvl.cost_amount, 100_000.0);
        let svc_row = comps.iter().find(|c| c.material_code == "DV1").unwrap();
        assert!(svc_row.is_service);
        assert_eq!(svc_row.quantity, 30.0);
        assert_eq!(svc_row.unit_cost, 0.0);
        assert_eq!(svc_row.cost_amount, 0.0);
    }

    /// Lô sản xuất định giá theo đơn giá KHAI trên định mức (kể cả dịch vụ),
    /// không lấy giá vốn FIFO; và snapshot không đổi khi định mức bị sửa sau.
    #[tokio::test]
    async fn lo_san_xuat_dinh_gia_theo_don_gia_khai_va_giu_snapshot() {
        let pool = test_pool().await;
        let (tp, _nvl) = seed_bom(&pool, 100.0).await;
        sqlx::query!(
            "UPDATE product_bom_item SET unit_price = 6000 WHERE product_id = ?",
            tp
        )
        .execute(&pool)
        .await
        .unwrap();
        let svc = seed_product(&pool, "DV1", "Nhân công", 0.0).await;
        sqlx::query!("UPDATE product SET is_service = 1 WHERE id = ?", svc)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO product_bom_item (product_id, material_product_id, quantity, unit_price)
             VALUES (?, ?, 3.0, 2000)",
            tp,
            svc
        )
        .execute(&pool)
        .await
        .unwrap();

        // Đơn giá nhập tay 8.000 bị thay bằng định mức: 2×6.000 + 3×2.000 = 18.000/SP.
        let r = save_inbound_core(
            &pool,
            "2026-03-15",
            "PNK-KHAI",
            "Nhập thành phẩm theo giá khai",
            "",
            "W1",
            "HKD",
            &[inp("TP1", 10.0, 8_000.0)],
            "",
            "production",
            "",
            0.0,
            "155",
            "154",
            false,
            "",
            true,
        )
        .await
        .expect("nhập sản xuất theo giá khai");

        assert_eq!(r.total, 180_000.0);
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT total FROM inbound_voucher WHERE voucher_no = 'PNK-KHAI'"
            )
            .await,
            180_000.0
        );
        // Bút toán nhập thành phẩm cũng dùng đơn giá khai, không dùng 8.000.
        let pn = sqlx::query!(
            "SELECT unit_price, amount FROM journal_entry
              WHERE voucher_no = 'PNK-KHAI' AND entry_type = 'PN'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pn.unit_price, 18_000.0);
        assert_eq!(pn.amount, 180_000.0);

        let comps = production_lot_components_core(&pool, "PNK-KHAI")
            .await
            .unwrap();
        let nvl = comps.iter().find(|c| c.material_code == "NVL1").unwrap();
        assert_eq!(nvl.unit_price, 6_000.0);
        assert_eq!(nvl.amount, 120_000.0);
        assert_eq!(nvl.unit_cost, 5_000.0);
        assert_eq!(nvl.cost_amount, 100_000.0);
        let svc_row = comps.iter().find(|c| c.material_code == "DV1").unwrap();
        assert!(svc_row.is_service);
        assert_eq!(svc_row.unit_price, 2_000.0);
        assert_eq!(svc_row.amount, 60_000.0);

        // Sửa định mức sau khi lưu → snapshot của lô cũ phải giữ nguyên.
        sqlx::query!(
            "UPDATE product_bom_item SET quantity = 9.0, unit_price = 1.0 WHERE product_id = ?",
            tp
        )
        .execute(&pool)
        .await
        .unwrap();
        let again = production_lot_components_core(&pool, "PNK-KHAI")
            .await
            .unwrap();
        let nvl_again = again.iter().find(|c| c.material_code == "NVL1").unwrap();
        assert_eq!(nvl_again.per_unit_qty, 2.0);
        assert_eq!(nvl_again.unit_price, 6_000.0);
        assert_eq!(nvl_again.quantity, 20.0);
        assert_eq!(nvl_again.amount, 120_000.0);
    }

    #[tokio::test]
    async fn khong_bat_o_tu_xuat_nvl_thi_khong_sinh_phieu_xuat() {
        let pool = test_pool().await;
        seed_bom(&pool, 100.0).await;

        let r = save_inbound_core(
            &pool,
            "2026-03-15",
            "PNK-KHONGO",
            "",
            "",
            "W1",
            "HKD",
            &[inp("TP1", 10.0, 8_000.0)],
            "",
            "production",
            "",
            0.0,
            "155",
            "154",
            false,
            "",
            false, // không bật ô tick
        )
        .await
        .expect("nhập sản xuất không tự xuất NVL");

        assert_eq!(r.material_voucher, "");
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PX'"
            )
            .await,
            0
        );
        // Tồn NVL giữ nguyên.
        assert_eq!(
            scalar_f64(
                &pool,
                "SELECT COALESCE(SUM(sl.quantity), 0) FROM stock_lot sl
                   JOIN product p ON p.id = sl.product_id WHERE p.code = 'NVL1'"
            )
            .await,
            100.0
        );
    }

    #[tokio::test]
    async fn pnk_san_xuat_thieu_nvl_thi_tra_luong_toan_bo() {
        let pool = test_pool().await;
        // Tồn 10 nhưng cần 20 → thiếu.
        seed_bom(&pool, 10.0).await;

        let err = save_inbound_core(
            &pool,
            "2026-03-15",
            "PNK-THIEU",
            "",
            "",
            "W1",
            "HKD",
            &[inp("TP1", 10.0, 8_000.0)],
            "",
            "production",
            "",
            0.0,
            "155",
            "154",
            false,
            "",
            true,
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho vật tư"), "{err}");

        // Rollback CẢ phiếu nhập: không PN, không PX, tồn không đổi.
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE entry_type IN ('PN', 'PX')"
            )
            .await,
            0
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM inbound_voucher WHERE voucher_no = 'PNK-THIEU'"
            )
            .await,
            0
        );
        assert_eq!(
            scalar_f64(&pool, "SELECT COALESCE(SUM(quantity), 0) FROM stock_lot").await,
            10.0
        );
    }
}
