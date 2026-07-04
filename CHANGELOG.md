## [1.12.3](https://github.com/RouHim/yummybox/compare/v1.12.2...v1.12.3) (2026-07-04)


### Bug Fixes

* run E2E container as root to avoid volume permission issues ([994f3fd](https://github.com/RouHim/yummybox/commit/994f3fd38471d6917323690ff98c3d3f2c565f74))

## [1.12.2](https://github.com/RouHim/yummybox/compare/v1.12.1...v1.12.2) (2026-07-04)


### Bug Fixes

* skip npm install in build.rs when web/build/ is pre-built ([12ea5a0](https://github.com/RouHim/yummybox/commit/12ea5a07116aa90b646e40a23184956ed282cb8a))

## [1.12.1](https://github.com/RouHim/yummybox/compare/v1.12.0...v1.12.1) (2026-07-04)


### Bug Fixes

* add explicit [[bin]] section so cargo fetch works without src/ present ([5f67f5f](https://github.com/RouHim/yummybox/commit/5f67f5fbe14cf73aedd8a97f9f102f98f484e302))

# [1.12.0](https://github.com/RouHim/yummybox/compare/v1.11.2...v1.12.0) (2026-07-04)


### Features

* add Docker container support with multi-arch GHCR release pipeline ([6744106](https://github.com/RouHim/yummybox/commit/6744106a8ffab53463eedbabe592617185f9a54a))

## 0.2.0 — YummyBox (2026-07-04)

### Changed
- Renamed application from MealMe to YummyBox. The `mealme` binary is now `yummybox`. Environment variables use the `YUMMYBOX_` prefix. On first run, an existing `meals.db` is automatically migrated to `yummybox.db`. See [README](https://github.com/RouHim/yummybox/blob/main/README.md) for upgrade notes.
- Changed default bind address from `127.0.0.1` to `0.0.0.0` so the app is reachable from Docker containers and other hosts on the network. **Security note**: on shared hosts, bind behind a reverse proxy or set a firewall rule to restrict access. The `YUMMYBOX_PORT` override still works as before.
- Added Docker/OCI container support with multi-arch images (`linux/amd64` + `linux/arm64`) published to GitHub Container Registry (`ghcr.io/rouhim/yummybox`). The image is `FROM scratch` with a statically linked musl binary running as non-root UID 1000. See the README Docker section for usage.

## [1.11.2](https://github.com/RouHim/mealme/compare/v1.11.1...v1.11.2) (2026-07-04)


### Bug Fixes

* paste tile click and Ctrl+V image paste across browsers ([96c26d9](https://github.com/RouHim/mealme/commit/96c26d9f62fd89373f0c12a081de24dfcc67a724))

## [1.11.1](https://github.com/RouHim/mealme/compare/v1.11.0...v1.11.1) (2026-07-04)

# [1.11.0](https://github.com/RouHim/mealme/compare/v1.10.0...v1.11.0) (2026-07-04)


### Bug Fixes

* update footer attribution test to target specific photographer link ([f9dda5c](https://github.com/RouHim/mealme/commit/f9dda5cb709288602fa1bfbcaa8e6ca86eb685f3))


### Features

* duplicate meal name detection + version endpoint + ImageInput component ([cb3af3b](https://github.com/RouHim/mealme/commit/cb3af3b8030ea8033a18ddea1e420d99e7102ce7))
* improve drag-drop UX and add drop-zone highlight test ([d8d1ac4](https://github.com/RouHim/mealme/commit/d8d1ac444240b49a4851eb252544d51b77acd71d))
* redesign image input as 4-tile reusable component ([79a8b58](https://github.com/RouHim/mealme/commit/79a8b58d55b72dc4eb2502fae541321b522acce4))

# [1.10.0](https://github.com/RouHim/mealme/compare/v1.9.0...v1.10.0) (2026-07-03)


### Features

* reorganize meals toolbar — direct Add meal button + More dropdown ([b74a728](https://github.com/RouHim/mealme/commit/b74a7283eb4ee3a80ba6787b1c60471fc230e275))

# [1.9.0](https://github.com/RouHim/mealme/compare/v1.8.0...v1.9.0) (2026-07-03)


### Features

* unify URL + Bulk URL import into single "Import URLs" tab ([0ac8023](https://github.com/RouHim/mealme/commit/0ac8023a147e197cd401d47aaaa3d0f51853e577))

# [1.8.0](https://github.com/RouHim/mealme/compare/v1.7.0...v1.8.0) (2026-07-03)


### Features

* edit meal from detail page stays on detail page ([cd53a1a](https://github.com/RouHim/mealme/commit/cd53a1ad66b84e9eb8ca03f1b0fc13e8ff45b35a))

# [1.7.0](https://github.com/RouHim/mealme/compare/v1.6.0...v1.7.0) (2026-07-02)


### Features

* add LLM polish button to improve cooking instructions ([66d900f](https://github.com/RouHim/mealme/commit/66d900f445763deb0a10504b36037c0a81fb55bb))

# [1.6.0](https://github.com/RouHim/mealme/compare/v1.5.1...v1.6.0) (2026-07-02)


### Features

* meal detail — edge-to-edge hero image with hover overlay actions ([cdf5a21](https://github.com/RouHim/mealme/commit/cdf5a214d4ffc2ad27564d20b9bc126d683747f1))

## [1.5.1](https://github.com/RouHim/mealme/compare/v1.5.0...v1.5.1) (2026-07-02)


### Bug Fixes

* remove stale photo hint from AI import placeholder text ([6903e02](https://github.com/RouHim/mealme/commit/6903e025f6d679903d5f0e14d753eea31a1fd64f))

# [1.5.0](https://github.com/RouHim/mealme/compare/v1.4.0...v1.5.0) (2026-07-02)


### Bug Fixes

* avoid effect_update_depth_exceeded in MealForm image preview ([4697b88](https://github.com/RouHim/mealme/commit/4697b88ee942deeb4d535da55cc846adc80dca7e))
* correct SvelteKit SSR config and i18n test assertion ([533c9d2](https://github.com/RouHim/mealme/commit/533c9d299ad1b76903bc249ab5cedbdcf3d22e4f))
* populate meal name on URL import and repair import panel reactivity ([a66d52c](https://github.com/RouHim/mealme/commit/a66d52cb033694fe124e62c665cab7c66791f897))
* restore planner weekday header grid layout ([fdf360e](https://github.com/RouHim/mealme/commit/fdf360e3a6ae6c4e407e280422ccb40c2245b7f4))


### Features

* image URL loading in meal form + recipe import, collapsible LLM settings ([701b8a8](https://github.com/RouHim/mealme/commit/701b8a840ad84ddd28c108730ca16e64503249bb))
* redesign add-meal popup with header, icon tabs, and collapsible import section ([91cabc1](https://github.com/RouHim/mealme/commit/91cabc19d73932284a2c76de0193d15c429ae765))
* user-selectable language dropdown (System / English / Deutsch) ([46a2bda](https://github.com/RouHim/mealme/commit/46a2bdaae8265354220cd99305ac9ef532ae6c82))

# [1.4.0](https://github.com/RouHim/mealme/compare/v1.3.2...v1.4.0) (2026-07-01)


### Bug Fixes

* Bring! API save-item uses correct JSON endpoint and user UUID header ([b98aa3e](https://github.com/RouHim/mealme/commit/b98aa3ea63e0d626b62b39a3ac81f37262781988))
* pin clock in planner past-week test to avoid date-dependent failure ([5356d20](https://github.com/RouHim/mealme/commit/5356d204344150fbb4b1c7af3a21fe455989ed19))
* preserve ingredient name casing on import and manual entry ([3bade2f](https://github.com/RouHim/mealme/commit/3bade2f71c9eec75d9c8e2cc4103862d6d5268b5))


### Features

* Bring! shopping list integration ([52436ff](https://github.com/RouHim/mealme/commit/52436ff66abfda08080ae5b9cef3e1b5e45eb4ec))
* bulk URL recipe import ([183616b](https://github.com/RouHim/mealme/commit/183616be67e6f9e609246350c256d87b7dadf3f2))
* cooking view, JSON-LD export, planner redesign, HTML sanitization ([1fd2002](https://github.com/RouHim/mealme/commit/1fd20022162fc0d0ef0974400fae9deda00369ee))
* extract image URLs from HTML for LLM import bare-URL hints ([6deca6c](https://github.com/RouHim/mealme/commit/6deca6c61d1cccb524087ad72fc59836c33be6de))
* LLM recipe import from URLs + UI polish ([2872911](https://github.com/RouHim/mealme/commit/2872911c91b9fd32e7d0e600b57761fa540bc1a1))
* remember latest LLM config via localStorage, polish import form contrast ([78fd1fb](https://github.com/RouHim/mealme/commit/78fd1fb1ad72ad9866292f1dfb8361cd57d97a64))

## [1.3.2](https://github.com/RouHim/mealme/compare/v1.3.1...v1.3.2) (2026-06-28)


### Bug Fixes

* equal-height meal cards in overview grid ([54ece7a](https://github.com/RouHim/mealme/commit/54ece7a972d58832c179d2bdaeb4848319f3cd85))

## [1.3.1](https://github.com/RouHim/mealme/compare/v1.3.0...v1.3.1) (2026-06-28)


### Bug Fixes

* align playwright version, install browsers from tests dir, use pre-built binary ([f03f02a](https://github.com/RouHim/mealme/commit/f03f02a8a5ecbd50e3176d32a491a096583b169a))
* hide 'not planned yet' footer on meal cards that haven't been planned ([3daedb6](https://github.com/RouHim/mealme/commit/3daedb6eb4f081ce682d1f0ba727dfd00d2b695e))
* use pre-built binary in e2e webServer, fallback to cargo run ([9a8cf08](https://github.com/RouHim/mealme/commit/9a8cf081fd7964651d74914be02485cdaf19c115))

# [1.3.0](https://github.com/RouHim/mealme/compare/v1.2.0...v1.3.0) (2026-06-28)


### Bug Fixes

* update i18n E2E tests for app-bar layout (no h1 on meals page) ([12b7a4b](https://github.com/RouHim/mealme/commit/12b7a4beb15e4799240d179f8455e1352617c363))


### Features

* main spacing — app-bar + footer glass, padding-bottom between content and footer ([cb23501](https://github.com/RouHim/mealme/commit/cb23501edf8faf8cd348170eea5248ef0aafd1ad))

# [1.2.0](https://github.com/RouHim/mealme/compare/v1.1.0...v1.2.0) (2026-06-28)


### Features

* replace README banner fork+knife with soup icon ([b9a1b9b](https://github.com/RouHim/mealme/commit/b9a1b9bd34d7dcc5a08797255678f0a94b043249))

# [1.1.0](https://github.com/RouHim/mealme/compare/v1.0.1...v1.1.0) (2026-06-28)


### Features

* replace default favicon with Lucide soup icon, add banner logo to all page headers ([fbc8688](https://github.com/RouHim/mealme/commit/fbc8688f3a34222063cebcc5e0cafab01fd90678))

## [1.0.1](https://github.com/RouHim/mealme/compare/v1.0.0...v1.0.1) (2026-06-28)


### Bug Fixes

* skip npm build in build.rs when web/build/ already exists ([d634ed2](https://github.com/RouHim/mealme/commit/d634ed2538bf10b0f37939d7d388012ab7d9c8d3))

# 1.0.0 (2026-06-28)


### Bug Fixes

* add missing MealForm.svelte component ([6051c55](https://github.com/RouHim/mealme/commit/6051c55ea49349c8c0e120a8e505f107e5afa9b4))
* remove 'All meals' label, fix theme toggle contrast ([5284082](https://github.com/RouHim/mealme/commit/5284082a03821de93c26a811991dc5027f1cf559))
* run npm ci before playwright install to match project version ([571c197](https://github.com/RouHim/mealme/commit/571c19738b7d2c9b9f1fe69ac5d506f655f3ea52))
* update E2E tests for redesigned multi-route app ([b04ba8c](https://github.com/RouHim/mealme/commit/b04ba8ce255624c264e8967fdfda0b6f4d08daa8))
* upgrade sqlx 0.8 → 0.9 to prevent WorkerCrashed on cancelled begin ([78aacd1](https://github.com/RouHim/mealme/commit/78aacd187a502dd9b209fc699f833f21859f7502)), closes [#3932](https://github.com/RouHim/mealme/issues/3932)
* use exact label matching for Name field in E2E tests ([6bf18d4](https://github.com/RouHim/mealme/commit/6bf18d4090193d75fdd17b3a5551f8f932821e4b))
* use GITHUB_TOKEN instead of RELEASE_TOKEN for semantic-release ([03095a5](https://github.com/RouHim/mealme/commit/03095a520d03fc80b851a3c79940d1a74c939ee7))


### Features

* add 'mealme seed' CLI subcommand ([7ed4823](https://github.com/RouHim/mealme/commit/7ed4823ed239b0e32364f313b32b746044d2c34b))
* convert meal cards to cooking-view links, drop lightbox ([925cf0d](https://github.com/RouHim/mealme/commit/925cf0d814aca40e5fdf23c673c90ba1931e9897))
* dual-theme color system, ambient photo background, theme toggle ([4b693f2](https://github.com/RouHim/mealme/commit/4b693f220c67654280af5006590981a90c080dfd))
* Editorial Kitchen visual redesign ([245f4ba](https://github.com/RouHim/mealme/commit/245f4ba6f36247af2a50ab014445d12d2cfe6f01))
* LLM recipe import via genai ([040588d](https://github.com/RouHim/mealme/commit/040588d3068ef23c7411965eb7ac1de4233219c8))
* meal images, planner UI polish, icon expansion ([dc792df](https://github.com/RouHim/mealme/commit/dc792df448f20d60e672fcce076c6451bc91ebd4))
* meal instructions, recipe import, glass-morphism UI refresh ([627fa33](https://github.com/RouHim/mealme/commit/627fa33a6c2a370c467d92a32279a8274e4a0402))
* **planner:** past-week dimming, meal count defaults to 3, locale toggle removal ([1db2f71](https://github.com/RouHim/mealme/commit/1db2f7160e7fcc28c3498ea7fa758ade99640adc))

# Changelog

All notable changes to this project will be documented in this file.
This file is auto-generated by semantic-release — do not edit manually.
