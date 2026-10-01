-- Địa điểm kinh doanh ghi trên đầu sổ theo mẫu TT 152/2025/TT-BTC
-- ("Địa điểm kinh doanh: …" / "Tên địa điểm kinh doanh: …").
--
-- Khác với `address` (địa chỉ đăng ký trên MST): hộ bán ở chợ, ở nhà khác thì
-- địa điểm ghi trên sổ là nơi bán thật. Để trống thì sổ tự lấy `address` để
-- không in ra dòng trống.
ALTER TABLE business ADD COLUMN location TEXT NOT NULL DEFAULT '';
