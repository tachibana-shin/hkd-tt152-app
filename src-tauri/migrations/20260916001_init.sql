-- HKD TT152 Database Schema
-- Based on Phan Mem Ke toan Excel_HKD - TT152.xlsx architecture
-- NOTE: journal_mode (WAL) + foreign_keys set via SqliteConnectOptions builder
-- (SQLx không nhận journal_mode/foreign_keys trong connection URL)

-- =============================================
-- CONFIG
-- =============================================

CREATE TABLE IF NOT EXISTS business (
    id                 INTEGER PRIMARY KEY DEFAULT 1,
    name               TEXT NOT NULL DEFAULT '',
    tax_code           TEXT NOT NULL DEFAULT '',
    address            TEXT NOT NULL DEFAULT '',
    short_name         TEXT NOT NULL DEFAULT '',
    ownership          TEXT NOT NULL DEFAULT 'Tư nhân',
    province           TEXT NOT NULL DEFAULT '',
    tax_code_issued_on TEXT NOT NULL DEFAULT '',
    phone              TEXT NOT NULL DEFAULT '',
    email              TEXT NOT NULL DEFAULT '',
    fiscal_year        INTEGER NOT NULL DEFAULT 2026
);

CREATE TABLE IF NOT EXISTS report_period (
    id        INTEGER PRIMARY KEY DEFAULT 1,
    from_date TEXT NOT NULL DEFAULT '',
    to_date   TEXT NOT NULL DEFAULT ''
);

-- =============================================
-- MASTER DATA (DANH MUC)
-- =============================================

CREATE TABLE IF NOT EXISTS business_unit (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS industry_group (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    code     TEXT UNIQUE NOT NULL,
    name     TEXT NOT NULL,
    vat_rate REAL NOT NULL DEFAULT 0.01,
    pit_rate REAL NOT NULL DEFAULT 0.005
);

CREATE TABLE IF NOT EXISTS expense_type (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tax_type (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS warehouse (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS account (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    code           TEXT UNIQUE NOT NULL,
    name           TEXT NOT NULL,
    opening_debit  REAL NOT NULL DEFAULT 0.0,
    opening_credit REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS product (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    code                 TEXT UNIQUE NOT NULL,
    name                 TEXT NOT NULL,
    unit                 TEXT NOT NULL DEFAULT 'Cái',
    sale_price           REAL NOT NULL DEFAULT 0.0,
    cost_price           REAL NOT NULL DEFAULT 0.0,
    min_stock            REAL NOT NULL DEFAULT 0.0,
    vat_rate             REAL NOT NULL DEFAULT 0.01,
    vat_reduced          BOOLEAN NOT NULL DEFAULT 0,
    default_warehouse_id INTEGER REFERENCES warehouse(id)
);

CREATE TABLE IF NOT EXISTS customer (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    code     TEXT UNIQUE NOT NULL,
    name     TEXT NOT NULL,
    address  TEXT NOT NULL DEFAULT '',
    tax_code TEXT NOT NULL DEFAULT '',
    phone    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS supplier (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    code     TEXT UNIQUE NOT NULL,
    name     TEXT NOT NULL,
    address  TEXT NOT NULL DEFAULT '',
    tax_code TEXT NOT NULL DEFAULT '',
    phone    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS employee (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    code       TEXT UNIQUE NOT NULL,
    name       TEXT NOT NULL,
    department TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS bank (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    code     TEXT UNIQUE NOT NULL,
    name     TEXT NOT NULL,
    branch   TEXT NOT NULL DEFAULT '',
    province TEXT NOT NULL DEFAULT ''
);

-- =============================================
-- JOURNAL_ENTRY (central hub — like NHAP LIEU sheet)
-- =============================================

CREATE TABLE IF NOT EXISTS journal_entry (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    posting_date    TEXT NOT NULL,
    voucher_no      TEXT NOT NULL,
    doc_date        TEXT NOT NULL DEFAULT '',
    description     TEXT NOT NULL DEFAULT '',
    product_code    TEXT NOT NULL DEFAULT '',
    supplier_code   TEXT NOT NULL DEFAULT '',
    customer_code   TEXT NOT NULL DEFAULT '',
    employee_code   TEXT NOT NULL DEFAULT '',
    quantity        REAL NOT NULL DEFAULT 0.0,
    unit_price      REAL NOT NULL DEFAULT 0.0,
    amount          REAL NOT NULL DEFAULT 0.0,
    entry_type      TEXT NOT NULL,
    debit_account   TEXT NOT NULL DEFAULT '',
    credit_account  TEXT NOT NULL DEFAULT '',
    bank_code       TEXT NOT NULL DEFAULT '',
    bank_direction  TEXT NOT NULL DEFAULT '',
    department      TEXT NOT NULL DEFAULT '',
    industry_code   TEXT NOT NULL DEFAULT '',
    vat_rate        REAL NOT NULL DEFAULT 0.0,
    pit_rate        REAL NOT NULL DEFAULT 0.0,
    tax_period      INTEGER NOT NULL DEFAULT 0,
    unit_code       TEXT NOT NULL DEFAULT '',
    sale_location   TEXT NOT NULL DEFAULT 'MatDat',
    adjust_code     TEXT NOT NULL DEFAULT '',
    note            TEXT NOT NULL DEFAULT ''
);

-- =============================================
-- STOCK_LOT (FIFO)
-- =============================================

CREATE TABLE IF NOT EXISTS stock_lot (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    product_id   INTEGER NOT NULL REFERENCES product(id),
    warehouse_id INTEGER NOT NULL REFERENCES warehouse(id),
    quantity     REAL NOT NULL DEFAULT 0.0,
    unit_cost    REAL NOT NULL DEFAULT 0.0,
    received_at  TEXT NOT NULL,
    depleted     BOOLEAN NOT NULL DEFAULT 0
);

-- =============================================
-- INVOICE
-- =============================================

CREATE TABLE IF NOT EXISTS invoice (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    number            TEXT NOT NULL DEFAULT '',
    date              TEXT NOT NULL,
    customer          TEXT NOT NULL DEFAULT '',
    customer_tax_code TEXT NOT NULL DEFAULT '',
    total             REAL NOT NULL DEFAULT 0.0,
    vat_amount        REAL NOT NULL DEFAULT 0.0,
    status            TEXT NOT NULL DEFAULT 'draft',
    e_invoice_no      TEXT NOT NULL DEFAULT '',
    e_invoice_symbol  TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS invoice_item (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id INTEGER NOT NULL REFERENCES invoice(id),
    product_id INTEGER NOT NULL REFERENCES product(id),
    quantity   REAL NOT NULL DEFAULT 0.0,
    unit_price REAL NOT NULL DEFAULT 0.0,
    subtotal   REAL NOT NULL DEFAULT 0.0
);

-- =============================================
-- INVENTORY_COUNT
-- =============================================

CREATE TABLE IF NOT EXISTS inventory_count (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS inventory_count_item (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    count_id     INTEGER NOT NULL REFERENCES inventory_count(id),
    product_id   INTEGER NOT NULL REFERENCES product(id),
    warehouse_id INTEGER NOT NULL REFERENCES warehouse(id),
    counted_qty  REAL NOT NULL DEFAULT 0.0,
    variance     REAL NOT NULL DEFAULT 0.0
);

-- =============================================
-- INDEX
-- =============================================

CREATE INDEX IF NOT EXISTS idx_je_posting_date ON journal_entry(posting_date);
CREATE INDEX IF NOT EXISTS idx_je_voucher_no ON journal_entry(voucher_no);
CREATE INDEX IF NOT EXISTS idx_je_entry_type ON journal_entry(entry_type);
CREATE INDEX IF NOT EXISTS idx_je_product_code ON journal_entry(product_code);
CREATE INDEX IF NOT EXISTS idx_je_industry_code ON journal_entry(industry_code);
CREATE INDEX IF NOT EXISTS idx_je_unit_code ON journal_entry(unit_code);
CREATE INDEX IF NOT EXISTS idx_je_tax_period ON journal_entry(tax_period);

-- =============================================
-- SEED DATA
-- =============================================

INSERT OR IGNORE INTO business (id) VALUES (1);
INSERT OR IGNORE INTO report_period (id) VALUES (1);

INSERT OR IGNORE INTO industry_group (code, name, vat_rate, pit_rate) VALUES
    ('PPHH', 'Phân phối, cung cấp hàng hóa', 0.01, 0.005),
    ('SX-CN', 'Sản xuất, chế biến, chế tạo', 0.03, 0.015),
    ('DVXD-KNL', 'Dịch vụ, xây dựng không bao thầu NVL', 0.05, 0.02),
    ('DVXD-KL', 'Dịch vụ, xây dựng có bao thầu NVL', 0.03, 0.015),
    ('VCTT', 'Vận tải, kho bãi', 0.03, 0.015);

INSERT OR IGNORE INTO expense_type (code, name) VALUES
    ('NhanCong', 'Tiền lương, phụ cấp, bảo hiểm...cho NLĐ'),
    ('NVL', 'Nguyên vật liệu, công cụ dụng cụ'),
    ('MuaNgoai', 'Chi phí dịch vụ mua ngoài'),
    ('KhauHao', 'Khấu hao TSCĐ'),
    ('LaiVay', 'Lãi vay'),
    ('Khac', 'Chi phí khác');

INSERT OR IGNORE INTO tax_type (code, name) VALUES
    ('GTGT', 'Thuế giá trị gia tăng'),
    ('TNCN', 'Thuế thu nhập cá nhân'),
    ('TNDoanhNghiep', 'Thuế thu nhập doanh nghiệp'),
    ('BaoVeMT', 'Thuế bảo vệ môi trường'),
    ('TaiNguyen', 'Thuế tài nguyên');

INSERT OR IGNORE INTO business_unit (code, name) VALUES
    ('HaNoi-01', 'Cơ sở 1 - Hà Nội');