import type { Employee, PayrollRow } from "@/types";

// Tỷ lệ trích BH theo bộ mẫu Excel (sheet Bang Luong / So Luong-BH)
export const EMPLOYER_BH_RATE = 0.215;
export const EMPLOYEE_BH_RATE = 0.105;
export const STANDARD_MONTH_DAYS = 26;

export interface PayrollPreview {
  timeSalary: number;
  allowance: number;
  gross: number;
  bhBase: number;
  bhEmployer: number;
  bhEmployee: number;
  net: number;
}

/** Tính nhanh các dòng tiền của một nhân viên trong kỳ lương (xem trước thời gian thực). */
export function previewPayroll(row: PayrollRow, emp?: Employee): PayrollPreview {
  const timeSalary = emp ? (emp.basic_salary / STANDARD_MONTH_DAYS) * row.work_days : 0;
  const allowance = emp ? emp.allowance_cv + emp.allowance_xx + emp.allowance_phone : 0;
  const gross = emp ? Math.round(timeSalary + allowance + row.bonus) : 0;
  const bhBase = emp ? (emp.bh_salary > 0 ? emp.bh_salary : gross) : 0;
  const bhEmployer = Math.round(bhBase * EMPLOYER_BH_RATE);
  const bhEmployee = Math.round(bhBase * EMPLOYEE_BH_RATE);
  const net = gross - bhEmployee - row.pit_amount - row.advance;
  return {
    timeSalary: Math.round(timeSalary),
    allowance,
    gross,
    bhBase,
    bhEmployer,
    bhEmployee,
    net,
  };
}
