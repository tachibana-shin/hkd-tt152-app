/**
 * Khung dữ liệu chi tiết hóa đơn (`detail_json`) — port từ `tra-cuu-hd`
 * (`server/hddt.gdt/details.ts`). Tên trường giữ nguyên theo cổng HĐĐT để
 * module Rust `packages/invoice-pdf/` khớp 100% với dữ liệu đã lưu trong DB.
 */

export interface TtKhac {
  ttruong: string;
  kdlieu: string | null;
  dlieu: string | null;
}

export interface ThTtlsuat {
  tsuat: string;
  thtien: number;
  tthue: number;
  gttsuat: unknown;
}

export interface TtttKhac {
  ttruong: string;
  kdlieu: string;
  dlieu: string;
}

export interface TtKhacKhac {
  ttruong: string;
  kdlieu: string;
  dlieu: string | null;
}

export interface HdHhdVuTtkhac {
  ttruong: string;
  kdlieu: string;
  dlieu: string | null;
}

export interface HdHhdVu {
  idhdon: string;
  id: string;
  dgia: number;
  dvtinh: string;
  ltsuat: string;
  sluong: number;
  stbchu: string | null;
  stckhau: number;
  stt: number;
  tchat: number;
  ten: string;
  thtcthue: number | null;
  thtien: number;
  tlckhau: number | null;
  tsuat: number;
  tthue: number | null;
  sxep: number;
  ttkhac: HdHhdVuTtkhac[];
  dvtte: string | null;
  tgia: number | null;
}

export interface InvoiceData {
  nbmst: string;
  khmshdon: number;
  khhdon: string;
  shdon: number;
  cqt: string;
  cttkhac: unknown[];
  dvtte: string;
  hdon: string;
  hsgcma: string;
  hsgoc: string;
  hthdon: number;
  htttoan: number;
  id: string;
  idtbao: string | null;
  khdon: string | null;
  khhdgoc: string | null;
  khmshdgoc: string | null;
  lhdgoc: string | null;
  mhdon: string;
  mtdiep: string | null;
  mtdtchieu: string;
  nbdchi: string;
  chma: string | null;
  chten: string | null;
  nbhdktngay: string | null;
  nbhdktso: string | null;
  nbhdso: string | null;
  nblddnbo: string | null;
  nbptvchuyen: string | null;
  nbstkhoan: string | null;
  nbten: string;
  nbtnhang: string | null;
  nbtnvchuyen: string | null;
  nbttkhac: TtKhac[];
  ncma: string;
  ncnhat: string;
  ngcnhat: string;
  nky: string;
  nmdchi: string;
  nmmst: string;
  nmstkhoan: string | null;
  nmten: string;
  nmtnhang: string | null;
  nmtnmua: string | null;
  nmttkhac: TtKhacKhac[];
  ntao: string;
  ntnhan: string;
  pban: string;
  ptgui: number;
  shdgoc: string | null;
  tchat: number;
  tdlap: string;
  tgia: number;
  tgtcthue: number;
  tgtthue: number;
  tgtttbchu: string;
  tgtttbso: number;
  thdon: string;
  thlap: number;
  thttlphi: unknown[];
  thttltsuat: ThTtlsuat[];
  tlhdon: string;
  ttcktmai: number;
  tthai: number;
  ttkhac: TtKhacKhac[];
  tttbao: number;
  ttttkhac: TtttKhac[];
  ttxly: number;
  tvandnkntt: string;
  mhso: string | null;
  ladhddt: number;
  mkhang: string;
  nbsdthoai: string;
  nbdctdtu: string;
  nbfax: string | null;
  nbwebsite: string;
  nbcks: string;
  nmsdthoai: string | null;
  nmdctdtu: string;
  nmcmnd: string | null;
  nmcks: string | null;
  bhphap: number;
  hddunlap: string | null;
  gchdgoc: string | null;
  tbhgtngay: string | null;
  bhpldo: string | null;
  bhpcbo: string | null;
  bhpngay: string | null;
  tdlhdgoc: string | null;
  tgtphi: number | null;
  unhiem: string | null;
  mstdvnunlhdon: string | null;
  tdvnunlhdon: string | null;
  nbmdvqhnsach: string | null;
  nbsqdinh: string | null;
  nbncqdinh: string | null;
  nbcqcqdinh: string | null;
  nbhtban: string | null;
  nmmdvqhnsach: string | null;
  nmddvchden: string | null;
  nmtgvchdtu: string | null;
  nmtgvchdden: string | null;
  nbtnban: string | null;
  dcdvnunlhdon: string | null;
  dksbke: string | null;
  dknlbke: string | null;
  thtttoan: string;
  msttcgp: string;
  cqtcks: string;
  gchu: string;
  kqcht: string | null;
  hdntgia: string | null;
  tgtkcthue: string | null;
  tgtkhac: string | null;
  nmshchieu: string | null;
  nmnchchieu: string | null;
  nmnhhhchieu: string | null;
  nmqtich: string | null;
  ktkhthue: string | null;
  hdhhdvu: HdHhdVu[];
  qrcode: string;
}
