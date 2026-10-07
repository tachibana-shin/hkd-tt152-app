# Changelog

## [0.20.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.20.0...v0.20.1) (2026-10-07)


### Bug Fixes

* **hddt:** default the lookup check-result filter to All ([7d67957](https://github.com/tachibana-shin/hkd-tt152-app/commit/7d679578ee39c7e90be2436548a910664f0225c9)), closes [#1](https://github.com/tachibana-shin/hkd-tt152-app/issues/1)
* **profile:** ask which profile to open at startup before auto login ([adf90f8](https://github.com/tachibana-shin/hkd-tt152-app/commit/adf90f8169a5a33909b9e173d0115a34862c1eeb)), closes [#2](https://github.com/tachibana-shin/hkd-tt152-app/issues/2)
* **profile:** reject duplicate profile names on create and rename ([672fe0d](https://github.com/tachibana-shin/hkd-tt152-app/commit/672fe0dd02d560632cea538cb55f95f429509f2d)), closes [#2](https://github.com/tachibana-shin/hkd-tt152-app/issues/2)

# [0.20.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.19.3...v0.20.0) (2026-10-06)


### Features

* **ui:** color invoice rows, add dashboard shortcuts, default auto-sync on ([43ecf8e](https://github.com/tachibana-shin/hkd-tt152-app/commit/43ecf8e7a12614d4c25e28b670cd48f8fb58d116))

## [0.19.3](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.19.2...v0.19.3) (2026-10-06)


### Bug Fixes

* **desktop:** open the native save dialog for every file export ([f19b742](https://github.com/tachibana-shin/hkd-tt152-app/commit/f19b742874d6c3ade50e4c680c0c2acc9d6bea7a))

## [0.19.2](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.19.1...v0.19.2) (2026-10-06)


### Bug Fixes

* **production:** list only BOM goods in lot dialog and stop dropdown overflow ([3d3c626](https://github.com/tachibana-shin/hkd-tt152-app/commit/3d3c62699c63b38d7cc4c092567a3337ff8e70c1))

## [0.19.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.19.0...v0.19.1) (2026-10-05)


### Bug Fixes

* **invoice:** load catalog when the draft dialog opens ([2e983cc](https://github.com/tachibana-shin/hkd-tt152-app/commit/2e983cc6239c25415e49aff25b093a1127335885))

# [0.19.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.18.0...v0.19.0) (2026-10-05)


### Features

* **invoice:** suggest a production lot when a built item is short ([3152243](https://github.com/tachibana-shin/hkd-tt152-app/commit/315224348bd2642d564b96459bc82eb38176422b))

# [0.18.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.17.1...v0.18.0) (2026-10-05)


### Features

* **inbound:** import stock lines from Excel with paged preview ([497d7c0](https://github.com/tachibana-shin/hkd-tt152-app/commit/497d7c0151fb44ba048c2f2afbb9dfa7a9bdef94))

## [0.17.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.17.0...v0.17.1) (2026-10-05)


### Bug Fixes

* **ui:** paginate client-side tables (bare paginator attr was ignored) ([2e81e71](https://github.com/tachibana-shin/hkd-tt152-app/commit/2e81e716ee183c17a319cfc8d0fea55ff8c36d5d))

# [0.17.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.16.0...v0.17.0) (2026-10-05)


### Bug Fixes

* **accounting:** drop the dead hidden table breaking the revenue card ([f66859a](https://github.com/tachibana-shin/hkd-tt152-app/commit/f66859ad0eb92b63b4470966becd81851b563dfd))
* **accounting:** drop zero-revenue rows from books and declarations ([44d9c1f](https://github.com/tachibana-shin/hkd-tt152-app/commit/44d9c1f590f1961cef3bc8d26297e2c774b5b2d4))
* **accounting:** fill default accounts on import and drop payable refunds from expense ([02ced1f](https://github.com/tachibana-shin/hkd-tt152-app/commit/02ced1f7e5e12c7b9e0ea4bb74c923c0164622c5))
* **accounting:** keep TT 152 book dates on one line in narrow windows ([cab2cd6](https://github.com/tachibana-shin/hkd-tt152-app/commit/cab2cd6de3336ef71f67153d0ab40952096245a1))
* **accounting:** let the TT 152 book fill its frame ([e9962d7](https://github.com/tachibana-shin/hkd-tt152-app/commit/e9962d70d7fcb696db5871f6c822f86c966df75e))
* **accounting:** make TT 152 book header and grid follow dark mode ([464c378](https://github.com/tachibana-shin/hkd-tt152-app/commit/464c378c2d4fccd9ce076d888996a3d3941c0800)), closes [#f4f4f5](https://github.com/tachibana-shin/hkd-tt152-app/issues/f4f4f5) [#f1f5f9](https://github.com/tachibana-shin/hkd-tt152-app/issues/f1f5f9) [#27272a](https://github.com/tachibana-shin/hkd-tt152-app/issues/27272a)
* **accounting:** scale TT 152 book columns so narrow windows stop overflowing ([ea58d98](https://github.com/tachibana-shin/hkd-tt152-app/commit/ea58d98971537a12c3d1e5cf024e203646cf1e97))
* **app:** show the real version in the sidebar footer ([23b4398](https://github.com/tachibana-shin/hkd-tt152-app/commit/23b4398b84cb348a7961ed8f7c9aed95bea4ec9c))
* **hddt:** no fake save error, reveal the saved password in place ([34effe2](https://github.com/tachibana-shin/hkd-tt152-app/commit/34effe220e1e9b3dc481e66d3c0db6592de04e54))
* **pdf:** align printed tables and tighten the invoice layout ([9623f2d](https://github.com/tachibana-shin/hkd-tt152-app/commit/9623f2debb41dd077093430a5ba41d79fc53833c))
* **pdf:** stop wrapping the last word of invoice info values ([7ee6546](https://github.com/tachibana-shin/hkd-tt152-app/commit/7ee65462b9145d34bfe091c42268518bee598fef))
* **profile:** read the data directory through one helper so nothing escapes HKD_DATA_DIR ([4ed816d](https://github.com/tachibana-shin/hkd-tt152-app/commit/4ed816d3f9f5fca8df43c80f72d816899c633a69))
* **release:** read the version from the workspace Cargo.lock ([142f911](https://github.com/tachibana-shin/hkd-tt152-app/commit/142f911c884655a890678e07c53aaf22ca569693))
* show trade discounts on vouchers and keep synced hddt columns frozen ([a434bc4](https://github.com/tachibana-shin/hkd-tt152-app/commit/a434bc4753daa151e775596a1a674139db391815))
* **ui:** find a product by code and say where a new alias went ([ce3d581](https://github.com/tachibana-shin/hkd-tt152-app/commit/ce3d581a71a68874254b827f91489d6030cda54a))


### Features

* **accounting:** add standard tax return form and tax period report export ([b2ef492](https://github.com/tachibana-shin/hkd-tt152-app/commit/b2ef492da0a8dc4c046bb8aef388b0988a9a366f))
* **accounting:** bump the exported Word book font to form size ([1bf336c](https://github.com/tachibana-shin/hkd-tt152-app/commit/1bf336cda06b973905c2afaf18382cf1d064b1f5))
* **accounting:** decouple the book period from the tax declaration ([3f86366](https://github.com/tachibana-shin/hkd-tt152-app/commit/3f86366f4456bcef6ac23dacf19fb3142f821a48))
* **accounting:** drop the redundant period labels on both book tabs ([ca2d94a](https://github.com/tachibana-shin/hkd-tt152-app/commit/ca2d94af52ca2c616db5341a541cbb4468e0ff58))
* **accounting:** export the viewed TT 152 book as an A4 .docx ([bcc586a](https://github.com/tachibana-shin/hkd-tt152-app/commit/bcc586aaaad728cb91e9ef8b307174d23ba05dee))
* **accounting:** export Word and Excel at a 14pt base font ([f6feefe](https://github.com/tachibana-shin/hkd-tt152-app/commit/f6feefe0e1362cbf09b039db37e8ecb4a49adac4))
* **accounting:** open the TT 152 book on the whole accounting year ([878cd5a](https://github.com/tachibana-shin/hkd-tt152-app/commit/878cd5a3cacdeac10c2ed2085dd77723324a3e32))
* **hddt:** accept any date as the e-invoice start date ([b2597b2](https://github.com/tachibana-shin/hkd-tt152-app/commit/b2597b2cb6151fba7987ff51bbc0076fd9313d23))
* **hddt:** download an invoice's XML from the invoice detail dialog ([87e4458](https://github.com/tachibana-shin/hkd-tt152-app/commit/87e445897042d49f3715a82b727916332ddf4338))
* **hddt:** keep e-invoice XML files downloaded from the tax portal ([122f990](https://github.com/tachibana-shin/hkd-tt152-app/commit/122f99062422dbe51c2207d9a56f00758f279136))
* **hddt:** keep e-invoice XML in SQLite and pack it into a ZIP for filing ([b65e739](https://github.com/tachibana-shin/hkd-tt152-app/commit/b65e739607245aa1395cb6644ecc666eb6f4654e))
* **invoice:** create a product quickly and add an alias on the line ([34d64b6](https://github.com/tachibana-shin/hkd-tt152-app/commit/34d64b6864efe5862219e0244409f308af1c67b8))
* **invoice:** preview a draft invoice as PDF from the export queue ([10b3283](https://github.com/tachibana-shin/hkd-tt152-app/commit/10b3283c55696cc4d38cbf811cc31fc9f8c35a10))
* **invoice:** quick lot creation and shortage confirmation in the draft dialog ([c60556d](https://github.com/tachibana-shin/hkd-tt152-app/commit/c60556d98efcc626c797b27ce234a83b8e0ee2e0))
* **invoice:** start a draft from the export queue and preview any invoice as PDF ([ecde0f8](https://github.com/tachibana-shin/hkd-tt152-app/commit/ecde0f8beb951092fd2aaf0d56a1ca119af12195))
* **pdf:** dash every inner grid line and fill the page with the watermark ([d35d6ce](https://github.com/tachibana-shin/hkd-tt152-app/commit/d35d6ce91cee02d39ae1bed48144653b9ff518e9))
* **pdf:** redesign the invoice layout around the goods table ([46a54b3](https://github.com/tachibana-shin/hkd-tt152-app/commit/46a54b3d683b59131ceec47ed84702cb80105bab))
* **pdf:** render every invoice type from its form code ([69b6f65](https://github.com/tachibana-shin/hkd-tt152-app/commit/69b6f65d530d460d96c4b9ba9242b1a8766714fe))
* **pdf:** render invoice PDFs in Rust with the htmltopdf engine ([2302137](https://github.com/tachibana-shin/hkd-tt152-app/commit/2302137c296cc6f8a0758cb83358fe229d0c1f7b))
* **product:** declare a bill of materials for many products on one screen ([5d7f5f6](https://github.com/tachibana-shin/hkd-tt152-app/commit/5d7f5f6b90ba8dd9e98a2733f1135436d970e717))
* **report:** fetch the period's missing XML and show a progress log while exporting ([ec57ddd](https://github.com/tachibana-shin/hkd-tt152-app/commit/ec57dddcdbd51062e1d0c2684bf5d50681b4b349))
* **settings:** check for updates and refresh e-invoices when the app opens ([39fd719](https://github.com/tachibana-shin/hkd-tt152-app/commit/39fd719fce7336b9d481e550bebf2c8b66428edc))
* **stock,invoice:** BOM auto-issue of materials and per-line product aliases ([61063bb](https://github.com/tachibana-shin/hkd-tt152-app/commit/61063bb484eebdf80ab52c4cb16e0b5f2050f63b))
* **stock:** create production lots from the bill of materials ([11ce567](https://github.com/tachibana-shin/hkd-tt152-app/commit/11ce567ae15e515f6f59e23d3d5bc9f2c624e787))
* **update:** open a dialog with the release log and an install button ([20fe292](https://github.com/tachibana-shin/hkd-tt152-app/commit/20fe2922cfac7a9e40d42ff00c5e59a3cf266e0c))
* **update:** show the release notes as markdown in the update dialog ([88f3a17](https://github.com/tachibana-shin/hkd-tt152-app/commit/88f3a178257b9e4d4e4866807f5617c234ffd762))
* view electronic purchase invoices as PDF ([7156bac](https://github.com/tachibana-shin/hkd-tt152-app/commit/7156bacadf8003d697131d308744cd92d1d6f40b))

# [0.16.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.15.1...v0.16.0) (2026-10-01)


### Features

* **accounting:** filter TT 152 books by household group and fill location line ([24eca88](https://github.com/tachibana-shin/hkd-tt152-app/commit/24eca884397cb97ceea40a45b9bdd1ae766bab7e))

## [0.15.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.15.0...v0.15.1) (2026-10-01)


### Bug Fixes

* **accounting:** post cost-of-sale 632/152 and align books with TT 152 forms ([7f3264d](https://github.com/tachibana-shin/hkd-tt152-app/commit/7f3264d1a2a9b94da6d9b8cf9a30b9ad260ca5c5))

# [0.15.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.14.0...v0.15.0) (2026-09-30)


### Features

* **accounting:** open journal and cash screens with the active period ([5cb1b8f](https://github.com/tachibana-shin/hkd-tt152-app/commit/5cb1b8f828b42ee3017f68696932a800326029ec))

# [0.14.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.13.0...v0.14.0) (2026-09-30)


### Features

* **accounting:** quick period switcher following the household tax period ([9b853c3](https://github.com/tachibana-shin/hkd-tt152-app/commit/9b853c32bba0ae0fddbbe1d75660e3b9569b36b9))

# [0.13.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.12.2...v0.13.0) (2026-09-30)


### Bug Fixes

* **e2e:** take the test data dir from TMPDIR instead of hard-coding /tmp ([cb90549](https://github.com/tachibana-shin/hkd-tt152-app/commit/cb90549d1a916596c5f194a6090d416da7635804))


### Features

* **accounting:** print each TT 152 book with its own official columns ([1d35306](https://github.com/tachibana-shin/hkd-tt152-app/commit/1d3530697131c3235d789c24b95cb353a8e62ccf))

## [0.12.2](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.12.1...v0.12.2) (2026-09-30)


### Bug Fixes

* **release:** read CHANGELOG.md from the tip of main when publishing ([1ab889e](https://github.com/tachibana-shin/hkd-tt152-app/commit/1ab889ec7cd52fc7e9189f6f5c685e124225f434))

## [0.12.1](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.12.0...v0.12.1) (2026-09-30)


### Bug Fixes

* **release:** include the change list in the published release body ([41d6e93](https://github.com/tachibana-shin/hkd-tt152-app/commit/41d6e93bd0a5cb2c67af5fb6f06ef7edc11928cf))

# [0.12.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.11.0...v0.12.0) (2026-09-30)


### Features

* **stock:** one row per voucher and persist the trade discount ([ceb0b00](https://github.com/tachibana-shin/hkd-tt152-app/commit/ceb0b00888f065a23d02c4d8d36055cc1655d6f8))

# [0.11.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.10.1...v0.11.0) (2026-09-29)


### Bug Fixes

* **tax:** keep the cumulative 1bn allowance for PIT while VAT stays on full revenue ([775018b](https://github.com/tachibana-shin/hkd-tt152-app/commit/775018b78f192864b50c5f17dc9239be0dcca8f8))


### Features

* **tax:** show the PIT deduction and the remaining allowance on the declarations ([e7987e6](https://github.com/tachibana-shin/hkd-tt152-app/commit/e7987e6b651f8cadeff604d7ab0a4e499fe3f46b))

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
