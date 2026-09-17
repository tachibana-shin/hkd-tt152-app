-- =============================================
-- 20260916004: Chấm công chi tiết theo ngày (sheet "Cham Cong")
-- Mã trạng thái (khớp ký hiệu bộ mẫu Excel):
--   X   công đủ (k/h "+")      NC  nửa công (k/h "1/2")
--   P   phép                    H   học tập
--   CT  công tác                TS  thai sản
--   O   ốm                      CO  con ốm
--   KL  không lương             ''  nghỉ (chưa chấm)
-- =============================================
CREATE TABLE IF NOT EXISTS attendance (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    work_date   TEXT NOT NULL,               -- 'YYYY-MM-DD'
    employee_id INTEGER NOT NULL REFERENCES employee(id) ON DELETE CASCADE,
    status      TEXT NOT NULL DEFAULT '',    -- X | NC | P | H | CT | TS | O | CO | KL | ''
    note        TEXT NOT NULL DEFAULT ''
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_attendance_emp_date ON attendance(employee_id, work_date);
CREATE INDEX IF NOT EXISTS idx_attendance_date ON attendance(work_date);