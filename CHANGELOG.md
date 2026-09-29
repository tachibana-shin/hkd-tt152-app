# Changelog

## [0.10.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.10.0...v0.10.1) (2026-09-29)


### Bug Fixes

* **tax:** a household confirmed as group 2 owes tax even below the revenue threshold ([5725901](https://github.com/tachibana-shin/hkd-tt152-app/commit/57259013fd352486aabd23ee047ffaebca223ee8))

# [0.10.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.9.3...v0.10.0) (2026-09-29)


### Bug Fixes

* **books:** ignore a crafted `back` query when returning from a book screen ([cd231fa](https://github.com/tachibana-shin/hkd-tt152-app/commit/cd231fae936cac4ead3f6a30d1afb00c41f18086))


### Features

* **books:** inherit the Kế toán HKD date range when opening a book and add a back button ([5539f6b](https://github.com/tachibana-shin/hkd-tt152-app/commit/5539f6b16bcd98504210328a8e79c82e4e2d8df7))
* **tax:** make revenue thresholds configurable and stabilise the household group per year ([6464a43](https://github.com/tachibana-shin/hkd-tt152-app/commit/6464a4319418cb4f6c69edace8773a689a2be4e5))

## [0.9.3](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.9.2...v0.9.3) (2026-09-29)


### Bug Fixes

* **tax:** no tax before the period that passes the threshold ([2ef6370](https://github.com/tachibana-shin/hkd-tt152-app/commit/2ef63708a0bb352cd81c94ff165595b1b2e4c139))

## [0.9.2](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.9.1...v0.9.2) (2026-09-29)


### Bug Fixes

* **tax:** warn when the household group contradicts its own revenue ([51c1efe](https://github.com/tachibana-shin/hkd-tt152-app/commit/51c1efea102bf79b1508095653daf5accd6ad45a))

## [0.9.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.9.0...v0.9.1) (2026-09-29)


### Bug Fixes

* **tax:** align the household tax return with the 2026 rules ([1354280](https://github.com/tachibana-shin/hkd-tt152-app/commit/135428071e2d6977cdbccea69a5b835cfe0709e3))

# [0.9.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.8.0...v0.9.0) (2026-09-29)


### Features

* paginate long lists on the server ([9462b3f](https://github.com/tachibana-shin/hkd-tt152-app/commit/9462b3f51a8398738c18a71a686feecf8e7a31df))

# [0.8.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.7.0...v0.8.0) (2026-09-28)


### Features

* **tax:** keep the household group in the business profile and pre-fill it ([8504424](https://github.com/tachibana-shin/hkd-tt152-app/commit/8504424bc0692a6d8ed54edd95939dcb767c0591))

# [0.7.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.6.1...v0.7.0) (2026-09-28)


### Features

* **accounting:** default the report dates to the household's tax period ([363bbda](https://github.com/tachibana-shin/hkd-tt152-app/commit/363bbda50999762489675e6747a58bfa1191341c))

Nhật ký thay đổi của HKD Kế Toán. Mỗi mục tương ứng một bản phát hành;
phiên bản được [semantic-release](https://github.com/semantic-release/semantic-release)
tính tự động từ commit trước đó (`feat` → minor, `fix` → patch, kèm `!` → major).

## [0.6.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.5.0...v0.6.0) (2026-09-28)


### Bug Fixes

* **ci:** format the files the formatter was rejecting ([aa8b0d0](https://github.com/tachibana-shin/hkd-tt152-app/commit/aa8b0d05918cbe5a7e3bd52c97b0763fa9ffab7d))
* **release:** make the version bump survive Windows and CI runners ([166bdf8](https://github.com/tachibana-shin/hkd-tt152-app/commit/166bdf803730007aea99b641021f3ffad6231b22))
* **release:** stop notarizing macOS builds when no Apple credentials exist ([2cc95be](https://github.com/tachibana-shin/hkd-tt152-app/commit/2cc95be1744570c8621177bbab6e2e44af9214a5))


### Features

* **release:** bump the version from the tag and keep a changelog ([74f6c0f](https://github.com/tachibana-shin/hkd-tt152-app/commit/74f6c0fc195f0d8260f7fd2a101bd7f8a03d9784))
* **release:** let semantic-release bump the version and keep a changelog ([61bcde3](https://github.com/tachibana-shin/hkd-tt152-app/commit/61bcde3d2d94843b2317fa9f186575a1914ac3d1))


## [0.6.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.6.0...v0.6.1) (2026-09-28)


### Bug Fixes

* **release:** skip packaging when there is nothing new to ship ([b48a14c](https://github.com/tachibana-shin/hkd-tt152-app/commit/b48a14c450ed326da34f50270500295a0e868c59))

## [0.3.0](https://github.com/tachibana-shin/hkd-tt152-app/tags/v0.3.0) (2026-09-27)

### Bug Fixes

- adjust window fitting and auto-dismiss toasts ([8fa10d3](https://github.com/tachibana-shin/hkd-tt152-app/commit/8fa10d3bc6182650dcecd11ae173256361ebd9c9))
- **ci:** grant semantic-release permission to tag, push changelog and open the release ([3b94405](https://github.com/tachibana-shin/hkd-tt152-app/commit/3b94405bf6f556c166654a9dd7f851c4b32d7904))
- **ci:** install sqlx-cli via cargo instead of an unresolvable action tag ([b58a9d0](https://github.com/tachibana-shin/hkd-tt152-app/commit/b58a9d0aef0f02437757830e93d0626aa78dfbb1))
- **ci:** pass --check to sqlx prepare before the cargo argument separator ([b2e648f](https://github.com/tachibana-shin/hkd-tt152-app/commit/b2e648fa5d87e698005e76f6b2dc08b35118d97f))
- **db:** update business unit name in migration ([3c9cf0e](https://github.com/tachibana-shin/hkd-tt152-app/commit/3c9cf0e2985337399d7700160a2fdc63efac9d2a))
- **hddt:** clamp invoice page size to the portal limit of 50 ([27f3ca4](https://github.com/tachibana-shin/hkd-tt152-app/commit/27f3ca4b08ec630008fe233b759524e0f029b6a7))
- **hddt:** rebuild invoice lookup on the portal's sold/purchase x invoice-type matrix ([78acf19](https://github.com/tachibana-shin/hkd-tt152-app/commit/78acf197f18423dd495a895b613082245fc20561))
- **hddt:** retry dropped connections before marking an invoice as manual ([f95938b](https://github.com/tachibana-shin/hkd-tt152-app/commit/f95938be40a23013fd1bf8516af38b77ba0b9991))
- **hddt:** retry failed invoice details within the same scan pass ([6c14d48](https://github.com/tachibana-shin/hkd-tt152-app/commit/6c14d48797cf870b0c6dd0c4437404c12bdbf672))
- **hddt:** send the numeric form code when fetching invoice details ([63a87d2](https://github.com/tachibana-shin/hkd-tt152-app/commit/63a87d2aeed2349d66fc806ebb398c13e53c7189))
- **hddt:** show the counterparty that matches the lookup direction ([547fbff](https://github.com/tachibana-shin/hkd-tt152-app/commit/547fbff50637a566559fafd333e8110a530d0608))
- **hddt:** use the portal's real field names for discount, unit and service ([0fbe684](https://github.com/tachibana-shin/hkd-tt152-app/commit/0fbe6845299005311ec27eb5bc89eca421c447da))
- **invoice:** keep tax out of the exported invoice ([6ec5d30](https://github.com/tachibana-shin/hkd-tt152-app/commit/6ec5d30b02f87e5b3788899976b379839d35dc6f))
- **invoice:** stop truncating product names in the export pack ([b084992](https://github.com/tachibana-shin/hkd-tt152-app/commit/b084992f62e4fd70ab46b6e60ca2033e26b626c3))
- **invoice:** sync the invoice date to the date of the issued e-invoice ([a0b5499](https://github.com/tachibana-shin/hkd-tt152-app/commit/a0b54997a7f634716b222885ae3673c7a5ef8563))
- **payroll:** include zero work days in attendance ([c37f060](https://github.com/tachibana-shin/hkd-tt152-app/commit/c37f0600677a6995c7c0b37ece1809e147d49518))
- primeui icon not show ([6fd5c54](https://github.com/tachibana-shin/hkd-tt152-app/commit/6fd5c5453944a76d8d5eda01e3c0fb2087972e5c))
- **ui:** stop showing UTC timestamps and UTC-parsed dates as local ones ([01cb9dc](https://github.com/tachibana-shin/hkd-tt152-app/commit/01cb9dc111118d3b213d40cbfe7313d65ef99ada))

### Features

- **accounting:** add trial balance & account CRUD ([7fff40b](https://github.com/tachibana-shin/hkd-tt152-app/commit/7fff40be76cf30bd634deda8491d0c3610c8ce29))
- add employee code generator and format VND ([1dd32cc](https://github.com/tachibana-shin/hkd-tt152-app/commit/1dd32cc6b5ab75e14d23aeae914456b30b62eacc))
- add profile picker and auto login ([5445385](https://github.com/tachibana-shin/hkd-tt152-app/commit/544538568a042bc4500331c4c700b1c2f1c40e74))
- **catalog:** add industry group management ([c898166](https://github.com/tachibana-shin/hkd-tt152-app/commit/c8981663911804e5fb05e8392a87869e87911248))
- **catalog:** bulk-assign the PPHH industry group and check queries at compile time ([11d197f](https://github.com/tachibana-shin/hkd-tt152-app/commit/11d197f16659f89f2155b56890c10cc5a798ca9b))
- **catalog:** create shared ProductSelect component ([880ee87](https://github.com/tachibana-shin/hkd-tt152-app/commit/880ee8769f329bdd3b8503f38573aec13fbd88b7))
- **catalog:** protect product integrity in books ([4708c78](https://github.com/tachibana-shin/hkd-tt152-app/commit/4708c78474aea9239e6d4c22851b28b37440cf5a))
- **catalog:** support service items and tax groups ([fe1e77b](https://github.com/tachibana-shin/hkd-tt152-app/commit/fe1e77b882dbc49d7356acf5040012b68934250b))
- **db:** seed default warehouse ([7b1b458](https://github.com/tachibana-shin/hkd-tt152-app/commit/7b1b458da250c32fe709bf4a6e62b9665469e402))
- **hddt:** allow scanning up to today while still not caching today ([80eae62](https://github.com/tachibana-shin/hkd-tt152-app/commit/80eae62b17c7a66366b18c3994d0ceb48ebf7cba))
- **hddt:** default synced goods to the PPHH industry group ([57cc36a](https://github.com/tachibana-shin/hkd-tt152-app/commit/57cc36a3c55a744f6eab0a55f9bfe97131d62b32))
- **hddt:** grow captcha templates to 1356 glyphs and fix the charset ([d27f0a2](https://github.com/tachibana-shin/hkd-tt152-app/commit/d27f0a2d4758e72554e5c8b7b71fc0c2defb616b))
- **hddt:** offline captcha auto-solve via glyph template matching ([b857285](https://github.com/tachibana-shin/hkd-tt152-app/commit/b8572856773da25943e7aeb29734935c553a06f7))
- **hddt:** remember lookup filters per tab and cache pages with KeepAlive ([72aafc7](https://github.com/tachibana-shin/hkd-tt152-app/commit/72aafc7a7c7252537b2eea0c5ab159f8fe6c4701))
- **hddt:** require a business start date for e-invoice use ([d178dcd](https://github.com/tachibana-shin/hkd-tt152-app/commit/d178dcded907089bfa7414c59f9e71db6c67fadc))
- **hddt:** rework HĐĐT portal integration (offline captcha, auto password change, reveal from DB) ([0620513](https://github.com/tachibana-shin/hkd-tt152-app/commit/062051314ba722ffad35025654661a4d17944699))
- **hddt:** sign in to the portal at app startup and prompt for captcha ([3e185b1](https://github.com/tachibana-shin/hkd-tt152-app/commit/3e185b11e9692a5310e19c4be32e642ca2aad931))
- **hddt:** split sync cache from imported invoices and add offline mocks ([b4cda61](https://github.com/tachibana-shin/hkd-tt152-app/commit/b4cda61f8557bfd534cd0d8c9d2c6ea3ab55ddeb))
- **hddt:** sync purchase invoices into linked inbound vouchers ([a34d0a8](https://github.com/tachibana-shin/hkd-tt152-app/commit/a34d0a84600aa4582bdaeea306f9bf8152d3e9b7))
- **hddt:** tra cứu hóa đơn theo format thật cổng HĐĐT ([726234e](https://github.com/tachibana-shin/hkd-tt152-app/commit/726234e0ebd4d464ba547ec8ec0998435beed992))
- **invoice:** add a queue of invoices still waiting to be issued ([3b63a9d](https://github.com/tachibana-shin/hkd-tt152-app/commit/3b63a9d8a97cffbd4970ce497f6a93a46dc3557c))
- **invoice:** allow deleting draft invoices ([b60de3e](https://github.com/tachibana-shin/hkd-tt152-app/commit/b60de3edf3e6be927b7d12d5f374d8c5c842e415))
- **invoice:** allow editing draft invoices ([a1fbe37](https://github.com/tachibana-shin/hkd-tt152-app/commit/a1fbe37fb19643d2b3e912326c26d3121fbcd477))
- **invoice:** create the outbound voucher when the e-invoice is recorded ([ca0d64b](https://github.com/tachibana-shin/hkd-tt152-app/commit/ca0d64bfe6d69ac9ba26b92373009ebb82a64b8a))
- **invoice:** outbound voucher auto-creates linked invoice ([17247cc](https://github.com/tachibana-shin/hkd-tt152-app/commit/17247cc537c07a93dbeafdd45262b7fd47182318))
- **invoice:** prepare invoices for issuing them in another e-invoice service ([cf6b2a8](https://github.com/tachibana-shin/hkd-tt152-app/commit/cf6b2a84dca58469807ce13edf9d593c24550be1))
- **invoice:** remember the household e-invoice symbol ([3c79612](https://github.com/tachibana-shin/hkd-tt152-app/commit/3c796120e1ed3e1080824a959a519948bdc5a13a))
- **invoice:** shape the copy block for the target cash register ([2d01c55](https://github.com/tachibana-shin/hkd-tt152-app/commit/2d01c55a5105703752a640df106b155b98052f21))
- **line-items:** add realtime stock shortage alerts ([35d8502](https://github.com/tachibana-shin/hkd-tt152-app/commit/35d8502e5d6f4640e8c65a689d3a50639de78297))
- remove invice tax ([9ec103d](https://github.com/tachibana-shin/hkd-tt152-app/commit/9ec103d9cdb59f8e394540d406cbe83a2e8d5320))
- **stock:** add a shared read-only voucher viewer for inbound and outbound ([35bbab5](https://github.com/tachibana-shin/hkd-tt152-app/commit/35bbab5360f29549737a2982a37f5d910fe01c54))
- **stock:** add invoice adjust flows (inbound & outbound) ([86df170](https://github.com/tachibana-shin/hkd-tt152-app/commit/86df170a9c89b5a7d55ba25ab8bbb184a9d3a7b6))
- **stock:** expand inbound flows per TT88 ([be25298](https://github.com/tachibana-shin/hkd-tt152-app/commit/be2529873a4f6cad1867b41e623d072a569691d1))
- **stock:** quick adjust voucher from original PNK/PXK row ([b419a46](https://github.com/tachibana-shin/hkd-tt152-app/commit/b419a460b04da645d17fae8ce58cdd4343c972a7))
- **stock:** support line-item warehouse allocation ([6a3a2c9](https://github.com/tachibana-shin/hkd-tt152-app/commit/6a3a2c97e8698085b81e0ef8317fe05dbb19ede7))
- **tax:** add tax code lookup integration ([40dc2b7](https://github.com/tachibana-shin/hkd-tt152-app/commit/40dc2b757adfac5df9b17f38d9437b19f3a98019))
- **theme:** add dark mode support ([3207520](https://github.com/tachibana-shin/hkd-tt152-app/commit/3207520784eafea98b8d05e95581bda1b96e562d))
- update ui ([309a316](https://github.com/tachibana-shin/hkd-tt152-app/commit/309a31600cfefcf17d3c33733f8dd6b92da6e900))
- **web:** add setting to toggle local web server ([c797d41](https://github.com/tachibana-shin/hkd-tt152-app/commit/c797d41b57655b4edef8fde29a36c099f430a392))

## [0.4.4](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.3.0...v0.4.4) (2026-09-28)

### Bug Fixes

- **ci:** correct semantic-release action inputs and skip packaging without a signing key ([c840628](https://github.com/tachibana-shin/hkd-tt152-app/commit/c8406284f31c4ba3bb33358133cd2049499d6980))
- **ci:** install sqlx-cli with the sqlite feature only (macros no longer exists in 0.9) ([928e605](https://github.com/tachibana-shin/hkd-tt152-app/commit/928e60519bedea7c095b2700f502e8e8794d2766))
- **release:** align the Tauri JS packages with the 2.12 crate ([a2bf27b](https://github.com/tachibana-shin/hkd-tt152-app/commit/a2bf27b457b98f9ca758fab9b0a8a65608a2ca8d))
- **release:** cancel a running packaging run instead of queueing behind it ([7ca6db9](https://github.com/tachibana-shin/hkd-tt152-app/commit/7ca6db907dd1d8228890d33f70fc82d144c16a49))
- **release:** install dependencies before bundling and use bash for the version step ([5e12fc4](https://github.com/tachibana-shin/hkd-tt152-app/commit/5e12fc4780ec6d5f9a8b796937f173b091315f61))
- **release:** resolve the packaged version from the pushed tag instead of an action output ([8d154fe](https://github.com/tachibana-shin/hkd-tt152-app/commit/8d154fe0e135b7be33dc7b8eeb69e66d28616465))
- **release:** run the version step under bash on Windows runners ([960f1d9](https://github.com/tachibana-shin/hkd-tt152-app/commit/960f1d9e0c7b46b8531a2fb7be2abea502926cbb))
- **release:** unblock on a retired macOS runner and cap packaging time ([f52d9d5](https://github.com/tachibana-shin/hkd-tt152-app/commit/f52d9d51b494cdde1a3200c7d5c948e14a067414))

### Features

- **release:** allow packaging a chosen version from the manual trigger ([cda2223](https://github.com/tachibana-shin/hkd-tt152-app/commit/cda22234ef02d7b9354cd77097774f78ef3e5a4c))

## [0.5.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.4.4...v0.5.0) (2026-09-28)

### Bug Fixes

- **release:** fall back to ad-hoc signing when no Apple certificate is configured ([31f6db8](https://github.com/tachibana-shin/hkd-tt152-app/commit/31f6db8e57a280928fff6d0a6a3b5283f26173cd))

### Features

- **branding:** name the installer and show the app icon in the UI ([7bbf2c5](https://github.com/tachibana-shin/hkd-tt152-app/commit/7bbf2c51069d4aab841f38acf70701a105cb6f21))
- **branding:** replace the default Tauri icon with an app icon ([4c2ebc5](https://github.com/tachibana-shin/hkd-tt152-app/commit/4c2ebc52abdad751a0ce605c8f9b18ab2fd74d00))
- **ui:** add a responsive layout for small windows and phones ([26b0e37](https://github.com/tachibana-shin/hkd-tt152-app/commit/26b0e37d10701dbe632b98d986f685fe89a6ecd5))
