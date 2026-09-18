#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};

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
    pub(crate) total: f64,
}

/// Lõi nhập kho (không phụ thuộc Tauri State → test trực tiếp):
/// tạo journal_entry (PN) + stock_lot (FIFO) trong 1 transaction.
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
) -> Result<InboundResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut total = 0.0;
    for item in items {
        let wh_id = resolve_warehouse(&mut tx, warehouse_code, &item.product_code).await?;
        let product_id = resolve_product(&mut tx, &item.product_code).await?.id;
        // tạo stock_lot FIFO
        sqlx::query!(
            "INSERT INTO stock_lot (product_id, warehouse_id, quantity, unit_cost, received_at, depleted)
             VALUES (?, ?, ?, ?, ?, 0)",
            product_id,
            wh_id,
            item.quantity,
            item.unit_price,
            posting_date
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        // ghi sổ nhập
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
            item.quantity * item.unit_price,
            "152",
            "331",
            "",
            0.0,
            0.0,
            unit_code,
            "",
            note,
        )
        .await?;
        total += item.quantity * item.unit_price;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(InboundResult {
        entries: items.len(),
        total,
    })
}

/// Kết quả lưu 1 phiếu xuất kho.
#[derive(Debug)]
pub(crate) struct OutboundResult {
    pub(crate) entries: usize,
    pub(crate) revenue: f64,
    pub(crate) cogs: f64,
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
) -> Result<OutboundResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut revenue_total = 0.0;
    let mut cogs_total = 0.0;

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
            revenue_total += amount;
            continue;
        }

        // Hàng hóa: FIFO lấy các lô chưa xuất hết theo ngày nhập
        let lots: Vec<(i64, f64, f64)> = sqlx::query_as(
            "SELECT id, quantity, unit_cost FROM stock_lot
             WHERE product_id = ? AND depleted = 0
             ORDER BY received_at ASC, id ASC",
        )
        .bind(product.id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let (cogs, takes) = allocate_fifo(&lots, item.quantity).map_err(|short| {
            format!(
                "Không đủ tồn kho cho '{}': thiếu {:.2} (còn thiếu sau khi xuất hết các lô)",
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
        revenue_total += amount;
        cogs_total += cogs;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(OutboundResult {
        entries: items.len(),
        revenue: revenue_total,
        cogs: cogs_total,
    })
}

/// Nhập kho: tạo journal_entry (PN) + stock_lot (FIFO) trong 1 transaction
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
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong phiếu nhập".into());
    }
    let unit_code = if unit_code.is_empty() {
        "HaNoi-01".to_string()
    } else {
        unit_code
    };
    let r = {
        let pool = state.pool.read().await;
        save_inbound_core(
            &pool,
            &posting_date,
            &voucher_no,
            &description,
            &supplier_code,
            &warehouse_code,
            &unit_code,
            &items,
            &note,
        )
        .await?
    };
    audit(&state, "save", "inbound", &voucher_no).await;
    Ok(json!({ "ok": true, "entries": r.entries, "total": r.total }).to_string())
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
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong phiếu xuất".into());
    }
    let unit_code = if unit_code.is_empty() {
        "HaNoi-01".to_string()
    } else {
        unit_code
    };
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
                je.quantity, je.unit_price, je.amount, je.debit_account, je.credit_account, je.note
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
        }
    }

    fn inp(product_code: &str, quantity: f64, unit_price: f64) -> InboundItemInput {
        InboundItemInput {
            product_code: product_code.into(),
            quantity,
            unit_price,
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
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w, 10.0, 1200.0, "2026-02-01").await;

        let r = save_outbound_core(
            &pool,
            "2026-03-10",
            "PXK-01",
            "Bán hàng",
            "",
            "HaNoi-01",
            &[out("P1", 15.0, 2000.0)],
            "",
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
        let (etype, debit, credit, amount, vat): (String, String, String, f64, f64) = sqlx::query_as(
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
            "HaNoi-01",
            &[out("P1", 8.0, 2000.0)],
            "",
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"));

        // Không có bút toán nào được ghi và lô giữ nguyên (rollback)
        assert_eq!(scalar_i64(&pool, "SELECT COUNT(*) FROM journal_entry").await, 0);
        assert_eq!(
            scalar_f64(&pool, "SELECT quantity FROM stock_lot").await,
            5.0
        );
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
            "HaNoi-01",
            &[OutboundItemInput {
                product_code: "S1".into(),
                quantity: 5.0,
                unit_price: 300_000.0,
                industry_code: "".into(), // rỗng → lấy nhóm ngành của sản phẩm
            }],
            "",
        )
        .await
        .expect("xuất dịch vụ không cần tồn kho");

        // Chỉ ghi doanh thu, không giá vốn, không đụng stock_lot.
        assert_eq!(r.entries, 1);
        assert_eq!(r.revenue, 1_500_000.0);
        assert_eq!(r.cogs, 0.0);
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM stock_lot").await,
            0
        );

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
            "HaNoi-01",
            &[inp("P1", 5.0, 1000.0), inp("P1", 3.0, 1200.0)],
            "",
        )
        .await
        .expect("nhập kho");

        assert_eq!(r.entries, 2);
        assert_eq!(r.total, 8600.0); // 5x1000 + 3x1200

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
}
