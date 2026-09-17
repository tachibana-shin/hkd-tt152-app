import { VerifyResult, PRODUCT_MAP } from "@primeui/license-manager";
import { join } from "path";

const verifyResult: VerifyResult = {
  valid: true,
  status: "active",
  message: "Hacked by Tachibana Shin",
  daysUntilExpiry: 9999999,
  payload: {
    id: crypto.randomUUID(),
    product: PRODUCT_MAP.primeui,
    tier: "commercial",
    type: "oem",
    iat: 0,
    exp: 9999999,
  },
};

const fileJs = join(import.meta.dirname, "node_modules/@primeui/license-manager/dist/index.mjs");

const code = await Bun.file(fileJs).text();
if (code.includes("Hacked by Tachibana Shin")) {
  console.log("Already patched! Skipping...");
  process.exit(0);
}

const patched =
  code.replace(/\w+ as verify,/, "").replace(/\w+ as verifyLicense/, "") +
  "\n" +
  `export async function verify() {
return ${JSON.stringify(verifyResult)};
}` +
  "\n" +
  `export async function verifyLicense() {
return ${JSON.stringify(verifyResult)};
}` +
  "\n//Hacked by Tachibana Shin";

await Bun.write(fileJs, patched);

console.log("Done! PrimeUI cracked by Tachibana Shin <tachibshin@duck.com>");
