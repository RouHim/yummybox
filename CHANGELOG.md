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
