-- =============================================
-- 20260916003: Bảng lương (Cham Cong / Bang Luong / So Luong-BH)
-- + Audit log (dấu vết thao tác)
-- =============================================

-- Mở rộng danh mục nhân viên theo sheet "Nhan Vien"
ALTER TABLE employee ADD COLUMN position       TEXT NOT NULL DEFAULT '';
ALTER TABLE employee ADD COLUMN job            TEXT NOT NULL DEFAULT '';
ALTER TABLE employee ADD COLUMN basic_salary   REAL NOT NULL DEFAULT 0;   -- Lương theo hợp đồng
ALTER TABLE employee ADD COLUMN allowance_cv   REAL NOT NULL DEFAULT 0;   -- Phụ cấp chức vụ
ALTER TABLE employee ADD COLUMN allowance_xx   REAL NOT NULL DEFAULT 0;   -- Phụ cấp xăng xe
ALTER TABLE employee ADD COLUMN allowance_phone REAL NOT NULL DEFAULT 0;  -- Phụ cấp điện thoại
ALTER TABLE employee ADD COLUMN bh_salary      REAL NOT NULL DEFAULT 0;   -- Lương đóng BH (nếu khác lương HĐ)
ALTER TABLE employee ADD COLUMN hired_on       TEXT NOT NULL DEFAULT '';
ALTER TABLE employee ADD COLUMN left_on        TEXT NOT NULL DEFAULT '';
ALTER TABLE employee ADD COLUMN dependents     INTEGER NOT NULL DEFAULT 0; -- Số người phụ thuộc (giảm trừ TNCN)

-- Bảng lương tháng (sheet "Bang Luong" / "So Luong-BH")
-- Quy ước tính (khớp bộ mẫu Excel):
--   Lương thời gian = basic_salary / 26 x Số công
--   Tổng (gross)   = Lương thời gian + phụ cấp (CV, xăng xe, ĐT) + thưởng
--   Căn cứ đóng BH  = bh_salary nếu > 0, ngược lại lấy gross
--   Trích BH tính vào CP HKD (NSDLĐ): BHXH 17,5% + BHYT 3,0% + BHTN 1,0% = 21,5%
--   Trích BH khấu trừ vào lương (NLĐ):  BHXH 8,0% + BHYT 1,5% + BHTN 1,0% = 10,5%
--   Thực lĩnh (net) = gross - BH người LĐ - thuế TNCN (nhập tay) - tạm ứng
CREATE TABLE IF NOT EXISTS payroll (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    period       TEXT NOT NULL,               -- 'YYYY-MM' (vd 2030-01)
    employee_id  INTEGER NOT NULL REFERENCES employee(id),
    work_days    REAL NOT NULL DEFAULT 0,     -- Số công tháng (chấm công)
    bonus        REAL NOT NULL DEFAULT 0,     -- Thưởng
    advance      REAL NOT NULL DEFAULT 0,     -- Tạm ứng / khấu trừ
    pit_amount   REAL NOT NULL DEFAULT 0,     -- Thuế TNCN (nhập tay theo biểu/QTT TNCN)
    gross_salary REAL NOT NULL DEFAULT 0,
    bh_employer  REAL NOT NULL DEFAULT 0,     -- Trích BH tính vào chi phí HKD (21,5%)
    bh_employee  REAL NOT NULL DEFAULT 0,     -- Trích BH khấu trừ vào lương (10,5%)
    net_pay      REAL NOT NULL DEFAULT 0,     -- Thực lĩnh
    note         TEXT NOT NULL DEFAULT '',
    UNIQUE(period, employee_id)
);

-- Nhật ký hoạt động (audit log — dấu vết các thao tác save_*/restore)
CREATE TABLE IF NOT EXISTS audit_log (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    ts     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    action TEXT NOT NULL,
    entity TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts);