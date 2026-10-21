# CLAUDE.md

Аддон Garry's Mod: Delirium (active ragdoll в стиле NaturalMotion) в режиме **FOLLOW** на форке физики
VPhysics-Box3D. Тело — `prop_ragdoll` Homer; Delirium каждый физический шаг читает части через `IPhysicsNM` и
возвращает силы, моменты и armature суставов. План — [ROADMAP.md](ROADMAP.md), решения — [docs/adr/](docs/adr/README.md).

С пользователем общаться **по-русски**.

## Жёсткие правила

- **Playroom (`delirium-room`) — golden** (ADR 0008). Homer в GMod должен вести себя один в один как в Playroom.
  **В работу Delirium не вмешиваемся**: те же входные данные (`fred.xrt`, `taskparams.txt`, build Homer, шаг 1/60,
  сообщения с теми же параметрами), drives как есть — без масштабирования, фильтров и своего тюнинга параметров NM.
  Единственное исключение — компенсация гравитации (ADR 0007). Если поведение расходится с Playroom, ищем
  расхождение во входных данных или в физике (чинится в форке), а не подкручиваем NM. Любое изменение тела или
  цикла подтверждается сверкой с Playroom (`tools/golden`: `NM_DUMP`, `--headless --script --rec`), не «на глаз».
  Пользовательский тюнинг поведений из меню (ADR 0009) — отдельная фича для игрока: только через публичный
  интерфейс NM (параметры сообщений, наборы `taskparams`), по умолчанию выключен (= Playroom), golden-прогоны
  всегда без него; расхождения с Playroom тюнингом не исправлять.
  Playroom собирается только так: `cargo build --release -p delirium-room --locked --manifest-path "<SDK>\Cargo.toml"
  --target-dir target-playroom` (в каталог SDK ничего не пишется).

- `E:\GitStuff\MyRepos\delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)` — **только чтение**, ничего там не
  менять, не собирать в его `target/`, не запускать `cargo fix`. Подключается path-зависимостью (ADR 0001).
- `E:\GitStuff\MyRepos\VPhysics-Box3D` (форк с `IPhysicsNM`, интерфейс `include/vphysics_nm.h`, `VBOX3D_NM_002`)
  **можно менять**, если там есть неточности, мешающие NM работать один в один как в Playroom (силы, armature,
  пивоты/лимиты суставов, порядок шага, единицы и т.п.). Правило: сначала показать расхождение с Playroom/SDK
  (что делает Delirium, что делает форк), потом чинить в форке; новые/изменённые методы — новой версией
  интерфейса (`VBOX3D_NM_003`…) с сохранением старых строк, заголовок копировать в модуль. Сборка форка —
  `cmake --preset windows-x64` + `cmake --build --preset windows-x64-release`, установка в
  `GarrysMod\bin\win64\vphysics.dll` (оригинал не трогать — `vphysics.dll.original`). Не связанное с NM в форке
  не менять.
- `E:\GitStuff\MyRepos\delirium-physics (gmod addon & api)` — прошлая попытка, только справочник, код не переносить.
  Её инструмент для переделки костей моделей игры (`delirium-ragdoll`: `--fit-model`, `--reskin`, `--nm-body`,
  `--template`, `.nmfit.txt`) **игнорировать полностью** — даже как справочник. Когда понадобятся модели, напишем
  новый инструмент с новым подходом.
- Только **win64** (`x86_64-pc-windows-msvc`, `gmsv_dms_win64.dll`). Никаких win32/x86/thiscall.
- **Без AVX2**: у пользователя процессор без AVX2 — никаких `target-cpu=x86-64-v3`, `+avx2`, `+fma`.
- Пока работаем **только с телом Homer**; модели ValveBiped отложены. Модель Homer импортируется заново тем же
  кодом, каким Delirium строит Homer (`delirium-data` + `delirium::ragdoll::Rig`), из копии GTA IV пользователя;
  наработки прошлой попытки (`delirium-ragdoll`, её `homer.mdl`) не использовать (ROADMAP M3).
- Игровые данные GTA IV и всё, что из них сделано (персонаж, build, модель Homer), в репозиторий и релизы не
  попадают (ADR 0006). Лицензия GPL-3.0-only.
- Каждая веха ROADMAP (M1, M2, …) делается **в отдельной новой сессии**.
- **Работа через PR.** В `main` напрямую не коммитить и не пушить. Каждая веха (или отдельная правка) — своя
  ветка от свежего `origin/main` (`git fetch` + `git switch -c <ветка> origin/main`), коммиты в неё, затем
  `git push -u origin <ветка>` и `gh pr create --base main` (repo `razedproduction/DMS-in-Gmod`). Пуш ветки и
  открытие PR входят в завершение вехи; мерж PR делает только пользователь. Сессии, работающие параллельно в одной
  папке, не переключают чужую ветку.

## Сборка, установка, проверка

```powershell
.\scripts\build.ps1            # -> dist\gmsv_dms_win64.dll (release; -DebugBuild для debug)
.\scripts\install.ps1          # dll -> garrysmod\lua\bin, addon\ -> garrysmod\addons\dms
```

- GMod: `E:\Games\Steam\steamapps\common\GarrysMod` (x86-64 ветка, форк `vphysics.dll` уже в `bin\win64`,
  оригинал — `vphysics.dll.original`). После сборки устанавливать в игру самому, не спрашивая.
- В PowerShell не запускать `build.ps1` с `2>&1`: вывод cargo в stderr превращается в ошибку.
- В игру Claude не заходит. Вне игры модуль проверяется загрузкой настоящего
  `garrysmod\bin\win64\lua_shared.dll` (`luaL_newstate` → `gmod13_open` через P/Invoke из PowerShell,
  `SetDllDirectory` на `bin\win64`); так же можно загрузить `vphysics.dll` форка и спросить `CreateInterface`.
  Такие скрипты держать в scratchpad, не в репозитории. Проверку в самой игре делает пользователь — дать ему
  точный список консольных команд.
- После M2.5 у Claude есть свой стенд: выделенный сервер GMod без окна (`scripts\server.ps1 start|stop|status`) и
  CLI `tools/dms-ctl` (`exec`, `lua`, `nm`, `watch --for`, `run <сценарий>`) — поведение и golden-сверку
  проверять там самому; пользователю оставлять только визуальную проверку. `watch` всегда запускать с `--for`
  (иначе команда не завершится). Канал `dms-ctl` — только `127.0.0.1` и только при `dms_dev 1`.

## Устройство

- `module/` — `gmsv_dms` (Rust, `cdylib`). `lua.rs` — LuaJIT API из уже загруженного `lua_shared.dll` через
  `GetProcAddress` (без крейтов-обёрток), нативы через `lua::native!` (`extern "C-unwind"`, ошибки → `lua_error`).
  `ffi/vphysics_nm.rs` — vtable `IPhysicsNM` строго в порядке объявления в заголовке.
- `addon/lua/` — серверный Lua: `autorun/server/dms_init.lua` (`require("dms")`), `dms/*.lua`.
- `scripts/`, `docs/adr/`, `tools/` (появится в M3).

Инварианты:
- Внутри step-callback (`SetStepCallback`, внутри `Simulate`) — **никакого Lua и игровых вызовов**, только методы
  `IPhysicsNM`; обмен с Lua через очереди команд/событий (ADR 0003).
- Без `IPhysicsNM` модуль загружается, но ничего не делает и объясняет почему (ADR 0005).
- Единицы на границе с NM — SI, Z вверх, 1 unit Source = 0.0254 м.
- **`sv_gravity` не меняем никогда** (сломает остальную игру). NM считает с 9.8 м/с² (`delirium::ragdoll::GRAVITY`),
  Source — 15.24 м/с²; разницу снимает сила `m_k · (g_nm − g_src)` на каждую часть через `ApplyBodyDrive`
  (ADR 0007). Drives NM не масштабировать.

## Стиль

- Код как в `module/src/*.rs`: краткие `///`-комментарии, `// SAFETY:` у каждого `unsafe`, без лишних зависимостей.
- Новое архитектурное решение — новый ADR `docs/adr/NNNN-slug.md` + строка в индексе; принятые ADR не
  переписывать, а заменять.
- Выполненные пункты вехи отмечать `[x]` в ROADMAP.md (в ветке вехи, вместе с её PR).
