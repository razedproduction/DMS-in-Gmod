# DMS in Gmod — Roadmap

Delirium (active ragdoll, NM-style) нативно в Garry's Mod в режиме **FOLLOW**: тело — это обычный `prop_ragdoll`
на физике VPhysics-Box3D, Delirium каждый физический шаг читает состояние частей и возвращает силы/моменты, а
армирование суставов (armature) решается внутри солвера Box3D.

## Исходные данные

| Компонент | Где | Что даёт | Правило |
|---|---|---|---|
| Delirium SDK **v0.2.2** (`91ee246`) | `../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)/crates/delirium-sdk` | C API `delirium_physics.h` (`nm_*`), Rust-крейт `delirium_physics` (`rlib` + `cdylib`) | **только чтение**, подключается path-зависимостью |
| VPhysics-Box3D (форк) | `vphysics/` (git subtree, ADR 0015) | `vphysics.dll` + `IPhysicsNM` (`VBOX3D_NM_004`, `vphysics/include/vphysics_nm.h`) | менять только ради совпадения с Playroom (ADR 0008), upstream — `git subtree pull` ([docs/vphysics.md](docs/vphysics.md)) |
| Документация Delirium (v0.2.2) | `../delirium-physics (…)/docs/how-it-works.md`, `docs/modding.md`, `CHANGELOG.md` | устройство движка, шаг, LEAD/FOLLOW, сообщения и их параметры, скрипты и `settings.txt` Playroom | справочник; где расходится с кодом — прав код Playroom |
| Прошлая попытка | `../delirium-physics (gmod addon & api)` | заметки и грабли (README) | только как справочник, код не переносим |

Ключевые факты из API, на которых строится план:

- FOLLOW-цикл: `GetBodyState` → `nm_agent_set_part_states` → `nm_agent_step(dt)` → `nm_agent_get_part_drives` →
  `ApplyBodyDrive` + `nm_agent_get_joint(...armature)` → `SetJointArmature`. Всё — внутри `SetStepCallback`,
  на главном потоке, внутри `IPhysicsEnvironment::Simulate`.
- Тело NM — **21 часть** (капсулы вдоль локальной Y), суставы ball/hinge с `pivot`, `frame`, `limits`, `armature`.
  Ragdoll ValveBiped — ~15 физобъектов. Форк рассчитан на соответствие **1 часть = 1 физобъект, 1 сустав = 1 constraint**.
- `SetJointAnchor` / `SetJointLimits` вызываются один раз при привязке, в позе покоя.
- NM живёт на 60 Гц, Source — 66 тиков (1/66 с, 4 сабстепа Box3D). SI-единицы, Z вверх, 1 unit = 0.0254 м.
- Delirium нужны данные персонажа (`fred.xrt` / `character.json`, `build.txt`, опционально `taskparams.txt`) из
  копии GTA IV пользователя — в аддоне их нет и быть не может.
- В FOLLOW `nm_agent_step` всё равно получает `NmWorld`: NM нужна геометрия мира (опора, raycast'ы, контакты частей).
- Шаг: `dt` ∈ (0, 0.25] с, внутри `round(dt·60)` сабстепов (1…8); родной шаг 1/60 — ровно один сабстеп. Поэтому
  NM всегда получает 1/60 (аккумулятор, M4), а не 1/66.
- Drives FOLLOW — без гравитации («don't add gravity compensation»): хост сам даёт телу 9.8 м/с², как Playroom.
  Наша компенсация (ADR 0007) снимает только разницу Source − 9.8, а не гравитацию NM.
- Feedback агента очищается в начале каждого шага NM: читать после каждого `nm_agent_step`, а не раз в тик.
- Спящий агент пропускает поведения, пока его не разбудят (`nm_agent_is_awake`).
- C API покрывает не всё, что делает Playroom: задачи движка (`shottask`, `meleetask`, `explosion`, `onfire`,
  `playerctl`), `extras.outbox` (сообщения, которые поведения шлют друг другу, напр. catchFall), реакции уровня
  комнаты — их вызывает код `delirium-room` напрямую через крейт `delirium`. Модуль делает так же (ADR 0016).

## Архитектура (целевая)

```
Lua (addon/)                         Binary module gmsv_dms (Rust)                         vphysics.dll (Box3D fork)
─────────────                        ──────────────────────────────                        ─────────────────────────
dms.Attach(rag) ──► DMS.* натив ──► Registry агентов ──► command queue ──┐
hooks (damage, remove)                 │                                   ▼
                                       │                  OnStep(env, dt) ◄── SetStepCallback ── Simulate()
                                       │                    GetBodyState ─────────────────────────► IPhysicsNM
                                       │                    nm_agent_step (delirium-sdk, rlib)
                                       │                    ApplyBodyDrive / SetJointArmature ───► IPhysicsNM
                                       └── events queue ──► Think hook ──► hook.Run("DMS_*")
```

Решения, принимаемые сразу (ADR в `docs/adr/`):

1. **Модуль на Rust**, SDK линкуется как `rlib` по пути — одна DLL, без отдельного `delirium_physics.dll`.
   Каталог SDK не трогаем: Cargo собирает его в `target/` этого репозитория.
2. **Только серверный модуль** (`gmsv_dms_win64.dll`; 32-битная ветка GMod не поддерживается). Клиенту хватает обычной
   сетевой репликации ragdoll'а; клиентский Lua — только отладка и UI.
3. **Lua никогда не вызывается из step-callback'а.** Lua → модуль: очередь команд, применяется в начале `OnStep`.
   Модуль → Lua: очередь событий, разбирается в `Think`.
4. **Только тело Homer.** Работаем с телом NM как есть (Homer): модель с 21 костью = 21 физобъект, кадр кости =
   кадр капсулы NM (нулевые смещения). Оригинальные модели GMod (ValveBiped) пока не делаем — см. «Отложено».
5. **Нет `IPhysicsNM` → аддон выключается** с понятным сообщением (никакого fallback на ванильную физику в FOLLOW).
6. Лицензия **GPL-3.0-only** (как у SDK). Игровые данные GTA IV не распространяются.
7. **Гравитация:** `sv_gravity` не трогаем, разницу с 9.8 м/с² NM снимает сила на каждую часть (ADR 0007).
8. **Playroom — golden** (ADR 0008): поведение Homer должно совпадать с Playroom один в один. В работу Delirium не
   вмешиваемся (те же входные данные, drives как есть, без своего тюнинга; исключение — гравитация); расхождения
   чиним в физике (форк), а проверяем автоматической сверкой с Playroom (`tools/golden`).

Структура репозитория:

```
DMS in Gmod/
  module/            gmsv_dms (Rust): ffi/vphysics_nm.rs, ffi/lua.rs, agent.rs, world.rs, bind.rs, step.rs
  tools/             dms-homer (Rust CLI): импорт Homer из копии GTA IV → homer.mdl/.phy под тело NM
                     golden (Rust CLI): прогон Playroom headless, сверка тела и траекторий с GMod
  golden/            scenarios/*.txt (сценарии); out/ — эталоны Playroom, локально, не в git
  addon/lua/         autorun/server/dms_init.lua, dms/*.lua, autorun/client/dms_debug.lua
  maps/ (опц.)       тестовая карта: пол, лестница, уступ, стена
  vphysics/          форк VPhysics-Box3D (git subtree, ADR 0015): vphysics.dll + IPhysicsNM
  docs/              adr/, data-setup.md, vphysics.md, api.md
  scripts/           build.ps1, build-vphysics.ps1, install.ps1 (копирование dll/аддона/форка в GMod)
```

---

## Этапы

Оценки — ориентировочные, в «рабочих днях одного человека». Каждый этап закрывается только по чек-листу DoD.

### M0. Фундамент — 1–2 дня

- [ ] `git init`, `.gitignore`, LICENSE (GPL-3.0), этот roadmap.
- [ ] ADR 1–6 (выше) записаны.
- [x] В форке VPhysics-Box3D закоммитить NM-расширение (сейчас `physics_nm.*`, `include/`, `cmake/patches/` не
      в git) отдельной веткой, собрать `windows-x64` (поддерживается только x86-64 ветка GMod). Закоммичено в
      razedproduction/VPhysics-Box3D (PR #1, #2), затем форк перенесён в `vphysics/` этого репозитория
      (`git subtree`, ADR 0015), сборка — `scripts\build-vphysics.ps1`, установка — `install.ps1 -Fork`.
- [ ] Скопировать `vphysics_nm.h` в `module/` как эталон для FFI (файл неизменяемый, версия `VBOX3D_NM_002`).
- [ ] `scripts/build.ps1` (cargo для `x86_64-pc-windows-msvc`, переименование в `gmsv_dms_win64.dll`),
      `scripts/install.ps1` (dll → `garrysmod/lua/bin`, аддон →
      `garrysmod/addons/dms`, `vphysics.dll` форка → `bin/win64` с бэкапом оригинала).
- [ ] Тестовый стенд: GMod x86-64 branch + форк + `sv_cheats 1`, `developer 1`.

**DoD:** пустой модуль собирается под win64, `require("dms")` в GMod печатает версию модуля и `nm_version()`.

### M1. Мост к VPhysics-Box3D — 3–4 дня

- [x] `ffi/vphysics_nm.rs`: `#[repr(C)]` vtable `IPhysicsNM` строго в порядке объявления (10 методов).
      x64 — обычный `extern "C"` с `this` первым аргументом (x86 не поддерживается, ADR 0002). `PhysicsNMBodyState_t`
      как `#[repr(C)]`.
- [x] Получение интерфейса: `GetModuleHandleA("vphysics.dll")` + `CreateInterface("VBOX3D_NM_002")`
      (Linux: `dlopen(..., RTLD_NOLOAD)`, `vphysics_srv.so` на dedicated — в M7). Если `NULL` — пробуем `_001`
      и предупреждаем, что лимитов суставов не будет; если и его нет — аддон выключен (`DMS.Available()` = false).
- [x] `lua.rs`: Lua userdata `PhysObj` → `IPhysicsObject*` (структура GMod `UserData { void* data; uint8 type; }`,
      проверка типа `PhysObj`). Без сторонних хаков — только через публичный layout.
- [x] Отладочные нативы: `DMS.Available()`, `DMS.InterfaceVersion()`, `DMS.BodyState(physobj)`,
      `DMS.Drive(physobj, force, torque[, steps])`, `DMS.FindJoint(a, b)`, `DMS.SetArmature(a, b, value)`,
      `DMS.GetArmature(a, b)`, `DMS.SubstepCount()`, `DMS.Hover(physobj, mass, g | false)`; консольные команды
      `dms_info`, `dms_bodystate`, `dms_hover`, `dms_armature` (`addon/lua/dms/debug_commands.lua`).
- [x] Step-callback на окружение сервера (`step.rs`): ставится при первом drive'е, снятие при выгрузке модуля
      (`gmod13_close`) и при `ShutDown` (`DMS.Shutdown()`).

**DoD:** (1) `BodyState` совпадает с `PhysObj:GetPos()/GetVelocity()` с учётом единиц и COM; (2) проп,
которому `Drive` даёт силу `m·g` вверх, висит в воздухе; (3) armature 1e6 на суставе ragdoll'а делает его жёстким,
0 — возвращает в норму; (4) `changelevel` и `lua_openscript` повторный `require` не роняют игру.

### M2. Delirium внутри модуля — 2–3 дня (параллельно с M1)

- [x] Path-зависимость на `delirium-sdk`, вызовы SDK как Rust-функций (обёртки с проверкой кодов `NM_E_*`
      и `nm_last_error`) — `module/src/nm.rs`.
- [x] ~~Загрузка данных из `garrysmod/data/dms/`~~ → по ADR 0010 данные читаются прямо из копии игры (конвар
      `dms_game_dir`) общим крейтом `tools/dms-game` — кодом Playroom: PC (`Fred.art` little-endian через
      `delirium_data::LE`, как в Playroom) и Xbox 360.
- [x] ~~Утилита «сделать `character.json`»~~ не нужна (ADR 0010); `docs/data-setup.md`.
- [x] Офлайн-самопроверка внутри модуля: агент в LEAD на плоском полу, 2 с `bodyBalance` — стоит
      (команда `dms_selftest`, натив `DMS.SelfTest`).
- [x] Справочные нативы: имена частей, список сообщений/параметров, `nm_build_part_info`, `nm_agent_get_joint`
      (`DMS.PartNames/Messages/MessageParams/BuildParts/BuildJoints/BuildDump`, команды `dms_nm_*`).
- [x] Входные данные NM — те же, что у Playroom (ADR 0008): build Homer = `NM_DUMP` байт в байт, связи из
      idle-клипа — бит в бит, агент в LEAD — бит в бит с Playroom (`golden dump|links|lead`).
- [x] Golden-стенд (`tools/golden`, ADR 0008, 0011): сборка Playroom из каталога SDK без записи в него (`--locked`,
      `--target-dir` в этом репозитории), `NM_DUMP` build'а Homer, прогоны по сценариям `golden/scenarios/`
      (стойка `bodyBalance` 60 с, выстрел в грудь, толчок, `catchFall` с высоты, `relax`; headless — бит в бит,
      остальные — `--script --rec` в окне, т.к. headless Playroom сценарии не исполняет) → `golden/out/` (не в
      git). Формат команд и записи — `golden/README.md`.

**DoD:** `dms_selftest` проходит в игре; без данных — внятная ошибка с путём, куда положить файлы; `tools/golden`
одной командой снимает эталоны Playroom для всех сценариев.

### M2.5. Тестовый Playroom на Source: стенд в игре + `dms-ctl` — 4–6 дней

Цель: стенд, где Claude сам, без пользователя, запускает Homer в GMod, управляет им и в реальном времени видит
его состояние — та же роль, что у Playroom `--headless --script --rec`, но на Source + Box3D. Пользователь
проверяет в игре только визуальное.

Сделано ([docs/bench.md](docs/bench.md), ADR 0012). Главное отличие от плана: **стенд — сама игра без окна**, а не
выделенный сервер — серверная сборка GMod x64 собрана с более новыми интерфейсами vphysics (`IAppSystem` с
`GetDependencies…`, длиннее `IPhysicsCollision`/`IPhysicsSurfaceProps`), и форк её роняет.

> **Решение (2026-10-21): от выделенного сервера отказались, все тесты — в игре.** Стенд — игра пользователя без
> окна (listen-сервер), `dms-ctl` подключается к ней. Порт форка на серверный ABI ради стенда не делается; к
> выделенному серверу возвращаемся только вместе с Linux/dedicated в M7, если он там понадобится.

- [x] ~~**Выделенный сервер GMod**~~ → стенд на игре (ADR 0012). Выделенный сервер поставлен
      (`E:\Games\GModServer`, `server.ps1 update`, `start -Dedicated`, `install.ps1 -Server`) и работает со своим
      `vphysics.dll` (канал, тестовый агент, записи), но **без форка**. Выделенный сервер для тестов **не
      используется** (решение выше); порт форка VPhysics-Box3D на серверный ABI (`IAppSystem` + интерфейсы vphysics)
      не делается — вопрос только для Linux/dedicated (M7).
- [x] **Запуск без окна:** `scripts\server.ps1 start|stop|restart|status` — `gmod_win64.exe -textmode`, окно
      скрывается, listen-сервер `maxplayers 2` + `-usercon` (RCON), `sv_hibernate_think 1`, `sv_cheats 1`, всё на
      `127.0.0.1`, RCON и токен канала — новые на каждый запуск, лог `-condebug` → `garrysmod\console.log`.
      Пока стенд запущен, пользователь не играет.
- [x] **Тестовая сцена:** `gm_flatgrass` — плоский пол, начало комнаты Playroom = пол под `info_player_start`
      (`dms_bench_scene`, `DMS.BenchScene()`). Отложено на M5: стадии Playroom (лестница, стена, уступ, мост) —
      вместе с миром NM из карты (в игре есть `vbsp/vvis/vrad`); спавн Homer в позе Playroom — M3/M4.
- [x] **Канал управления и телеметрии в модуле** (`dev.rs`, `dev.lua`): только `127.0.0.1`, только при `dms_dev 1`
      (включает лишь одноразовый `data/dms/bench.txt` от `server.ps1`), токен; сетевые потоки только очередь и
      готовые кадры (ADR 0003), исполнение в `Think`. Телеметрия — JSON-строка на кадр сервера: время/тик,
      drives `step.rs`, гравитации NM/Source (компенсация — `null` до M4), место под FOLLOW-агентов (`agents`, M4);
      тестового агента — шаг, флаги `balancing/fallen`, COM, части, контакты, drives, feedback.
- [x] **CLI `tools/dms-ctl`**: `status`, `exec` (RCON с выводом; `--module`), `lua` (`-f`), `nm`, `bench`,
      `snapshot`, `watch --for … [--fields] [--every]`, `run <сценарий>` → `.rec` Playroom; `lua` уходит в RCON
      `lua_run`, `exec` — в модуль, если другой путь не отвечает.
- [x] **Сверка в один шаг:** `golden behaviour --source [имена]` — сценарии на стенде против `golden/out/`.
      Пока сценарии исполняет агент SDK в LEAD (как `dms_selftest`), т.е. проверяется конвейер, а не Box3D:
      `stand` (60 с), `stand-1s`, `relax`, `relax-1s` совпадают с Playroom до 3 знаков записи на всех шагах. Не
      совпадают, и это известно: `shot` — с шага 62 (выстрел Playroom — ShotTask, M6); `push` — тычок от камеры
      Playroom, которая в окне движется (стенд берёт её начальную точку); **`catchfall` — Playroom пересылает агенту
      сообщения его `extras.outbox` (catchFall сам запускает `pedalLegs`, `bodyRollUp`), а C API SDK этот outbox
      не отдаёт — решить в M4** (иначе FOLLOW-агент не совпадёт с Playroom на catchFall). Осмысленная сверка
      поведения Homer — после M3 (модель) и M4 (FOLLOW).
- [x] Время: прогоны в реальном времени (тики игры, шаги NM 1/60 по аккумулятору, как окно Playroom);
      `host_timescale` — только для отладки.

**DoD:** одной командой (`scripts\server.ps1 start`) поднимается игра без окна (listen-сервер); `dms-ctl exec status`,
`dms-ctl lua "return DMS.Version"` и `dms-ctl watch --for 2s` работают из PowerShell; сценарий из
`golden/scenarios/` прогоняется в игре и даёт запись в формате Playroom; всё доступно из сессии Claude без
участия пользователя.

### M3. Модель Homer: импорт с нуля под NM — 6–9 дней

Цель: ragdoll Homer, у которого физика совпадает с телом NM, чтобы любые проблемы FOLLOW были проблемами
интеграции, а не подгонки. Дальше все вехи работают только с ним (дальше «эталон» = Homer).

Модель **импортируется заново**, ничего не берётся из прошлой попытки (её `delirium-ragdoll`, `homer.mdl` и
`.nmfit.txt` не используются). Принцип — максимальная совместимость с NM: всё, что касается тела, берётся из тех же
данных и тем же кодом, каким Delirium строит Homer сам (`delirium-data` + `delirium::ragdoll::Rig`, как
`getup::homer` в Playroom), без подгонок (`nm_build_fit`) и ручных правок.

Сделано ([docs/homer.md](docs/homer.md), ADR 0013). Отличия от плана: кадр физобъекта — кадр части NM, но ось
капсулы в нём повёрнута (NM поворачивает форму своим link'ом), ступни и кисти — боксы NM; вид — как в Playroom
(капсулы и боксы NM, без меша и текстур GTA IV).

- [x] `tools/dms-homer` (Rust CLI): Homer из копии GTA IV через `tools/dms-game` (тот же build, что у Playroom, без
      `nm_build_fit`); тело NM — общий `dms_game::body` (`Ragdoll` + `Agent::joints`), его же берут модуль и `golden`.
- [x] Физика: 21 физобъект в порядке частей NM, кость повёрнута как кадр части NM, её начало — в пивоте сустава
      (Source рисует кость ребёнка на постоянном смещении от родителя), центр масс — в центре части, оболочки капсул/боксов NM (уменьшены на
      6.35 мм — форк их раздувает; шаг ≥ 1.4 см — форк сваривает ближе 1 см), массы NM точно, центр масс в центре
      части и диагональ инерции NM — прямо в `.phy`, ball/hinge по NM, только пары self-collision NM, демпфирование 0.
- [x] Визуал — как рисует Homer Playroom (`part_mesh`, `part_color`): капсула/бокс NM каждой части в её кадре
      капсулы, левые части красные, правые синие, остальные серые, полупрозрачные; остальные схемы `PED_COLORS`
      (all blue, all red, solid blue, mixed opaque) — скины модели (`$texturegroup`). Меш и текстуры GTA IV не
      экспортируются. Лицо и скелет-оверлей Playroom — M6.
- [x] Сборка: SMD + `.qc` → `studiomdl` GMod (`bin\win64`, с форком) → `garrysmod/models/dms/homer.mdl` + `.phy`
      (текст переписывается: солиды по индексам — так их берёт игра), материалы в `materials/models/dms/homer/`.
- [x] Модель и рабочие файлы — только у пользователя (`target/dms-homer/`, папки GMod), в git не попадают.
- [x] Проверка в игре: 21 физобъект, массы, все 20 суставов через `FindConstraint` (`golden body`).
- [x] Bind (`bind.rs`, `DMS.BindHomer`): `SetJointAnchor` + `SetJointLimits` (кадры NM на родителе и ребёнке) на
      каждом суставе; части → физобъекты по именам костей (`DMS_HomerParts`).
- [x] Спавн: `dms_homer` — Homer в стоячей позе Playroom перед игроком, лицом к нему, сразу связан с NM.
- [x] **`golden body`**: build = `NM_DUMP` (байт в байт; поля и C API SDK — 0…5e-7); `.phy` — массы/центр масс/
      диагональ инерции точно, оболочки −3.1…+0.5 мм, объём ≤ 3.7 %, констрейнты и пары NM; в игре — позиции
      ≤ 0.6 мм, повороты ≤ 0.1°, массы и диагональ инерции точно, падение без NM — без взрывов, пивоты ≤ 1.5 мм.
- [x] **Расхождения форка исправлены** (ADR 0014, `VBOX3D_NM_003`): полный тензор инерции (`SetBodyInertia`; был
      только диагональ, у рук до 46 %) и шар с семантикой NM (`SetJointBallLimits`: twist по Z, эллипс наклонов в
      кадре родителя — патч Box3D `box3d-elliptical-cone.patch`; было twist/cone/plane с эвристической осью и
      перепутанными X/Y, бедро уходило на ~15°). `golden body` проходит: тензор 7e-8, суставы при падении ≤ 0.5° за
      пределами NM, пивоты ≤ 0.6 мм.
- [ ] Визуальная проверка пользователем (docs/homer.md: вид, `vcollide_wireframe 1`, падение).
**DoD:** `dms-homer` из копии GTA IV одной командой делает `homer.mdl`; `tools/golden body` проходит: build совпадает
с Playroom (1e-5), у заспавненного Homer все 21 часть совпадают с Playroom (позиция ≤ 1 мм, поворот ≤ 0.5°, масса
точно, инерция ≤ 1 %); брошенный без NM он ведёт себя как обычный ragdoll, суставы гнутся в пределах NM, ничего не
взрывается; лимиты видны через `IPhysicsConstraint::GetConstraintTransform`.

### Обновление SDK: Delirium v0.1.9+7 → v0.2.2 (2026-10-26)

- [x] SDK на `v0.2.2` (`91ee246`); C API (`delirium-sdk`) не менялся, правок API не понадобилось. `bevy_math` /
      `bevy_transform` 0.18 → 0.20 (`glam` 0.30 → 0.33, версии — как в `Cargo.lock` SDK). Сборка — без AVX2: у модуля
      те же рантайм-ветки с проверкой CPU (`ymm`/BMI в memchr и std), что и до обновления, FMA-инструкций нет.
- [x] Тело Homer не изменилось: сужающиеся капсулы (`Shape::Tapered`, `8c47480`) есть только в GJK, частям Homer они не
      назначаются; build отличается от прежнего в последнем бите float (≤ 1e-7, новый `glam`). Модель `dms-homer`
      пересобрана (формы и `.phy` те же, вершины оболочек сдвинуты на тысячные доли мм).
- [x] `tools/dms-game` сверен с Playroom: `getup::{key, plat, homer, Clips::stand_up}` и порядок `load` те же (в
      `Clips::load` добавились только клипы плавания). Изменился пол комнаты Playroom: `world::floor_tris(pool, 50)` —
      плитки 1 м, как прежний `add_floor`, но с ямой бассейна (x −3…3, y 5…25 м); `dms_game::floor` повторяет его
      (мир `dms_selftest` и стенда, `golden lead`).
- [x] Эталоны Playroom сняты заново (`golden/out/`): поведение изменилось (стойка расходится с прежней с ~0.87 с,
      `catchFall` теперь шлёт `stopAllBehaviours` в 0.67 с вместо `pedalLegs`/`bodyRollUp` и т.д. — «tester fixes»).
      Окно Playroom с v0.2.1 читает `%APPDATA%\delirium\settings.txt` (там и переключатели поведения) и на обычной
      графике идёт < 10 FPS (теряет шаги), поэтому `golden run` запускает окно с `--safe`: умолчания Playroom и
      минимальная графика, 60 шагов/с.
- [x] `golden`: dump байт в байт, links и lead бит в бит, `body` проходит целиком; `behaviour --source` — `stand`
      (60 с), `stand-1s`, `relax`, `relax-1s` совпадают до 3 знаков на всех шагах. Как и раньше, не совпадают `shot`
      (M6), `push` (камера Playroom; COM теперь в пределах 1.4 мм) и `catchfall` (outbox, M4).

### M4. FOLLOW-цикл на эталоне — 5–8 дней (главная веха)

Сделано (ADR 0017). Главное отличие от плана: агент FOLLOW работает не через C API, а на крейте `delirium`, как
Playroom (outbox, `in_air`, импульсы NM мимо `doll.out`), и понадобились правки физики форка — без них Homer падал за
~2 с.

- [x] `step.rs` + `follow.rs`: на каждый шаг NM — команды → `GetBodyState` ×21 → `set_body` → слой комнаты Playroom →
      `Agent::step_follow` → drives (`doll.out`) → `ApplyBodyDrive` ×21 на каждый шаг Source → armature из
      `doll.armature` → `SetJointArmature` ×20. Внутри step-callback — только `IPhysicsNM` (ADR 0003). Кадр тела Homer
      = кадр части NM (M3), центр масс в центре части — преобразования не нужны.
- [x] Время: NM всегда 1/60 (аккумулятор над шагами физики 0.015 с, drives держатся до следующего шага NM). Проверено:
      `-tickrate 60` шаг физики не меняет (игра ставит 0.015), шаг физики 1/60 (`DMS.EnvTimestep`) и шаг NM = шагу
      Source — не лучше. Запись сценария — в порядке тика Playroom.
- [x] Гравитация (ADR 0007): `sv_gravity` не трогаем, `F_k = m_k · (g_nm − g_src)`, `g_src` из `physenv.GetGravity()`;
      режимы `dms_gravity_mode nm | hybrid | off` замерены против Playroom, **по умолчанию `nm`** (решение
      пользователя): только с ним все сценарии в допусках.
- [x] Сообщения между поведениями: `extras.outbox` пересылается после каждого шага NM как в Playroom (`extras_frame`).
      В v0.2.2 `stopAllBehaviours` в `catchfall` шлёт `in_air` слоя комнаты — перенесён тем же кодом (ADR 0016, 0017).
- [x] Feedback и флаги — после каждого шага NM в очередь событий (`DMS.FollowEvents`, хуки `DMS_Feedback`,
      `DMS_Fallen`, `DMS_Detached`).
- [x] Сон: собственный сон доллы NM сохраняется, пока Box3D не сдвинул тело; тела засыпают вместе с ней (событие
      `sleep`), удар будит обоих.
- [x] Защита: NaN/Inf или разнос (> 80 м/с, > 300 рад/с) → агент снимается, armature 0, ragdoll обычный.
- [x] Отладка: `dms_status`, `dms_follow_send`, телеметрия `dms-ctl watch` (`agents`: шаг, флаги, COM, части, drives,
      armature, `pred_err` — расхождение Box3D с предсказанием физики NM, `limit_use`, `state_v`, компенсация,
      стоимость NM). Клиентский оверлей капсул/сил **не сделан** — перенесён в M6 (вместе с лицом и скелетом Homer).
- [x] Golden-сверка: сценарии стенда исполняет Homer в FOLLOW на Box3D (`--lead` — прежний LEAD-агент), команды —
      на шагах Playroom из меток его записи; `golden behaviour --source` проверяет допуски сценария (`#! tol`).
- [x] Правки форка (ADR 0017, `VBOX3D_NM_003` без изменения интерфейса): armature (`Cdot0` раз на сабстеп, 8
      проходов Гаусса–Зейделя по суставам с armature), скорость начала шага в `GetBodyState`, непрерывная гравитация
      тел NM. `golden body` проходит.
- [ ] Известные расхождения (план — следующий шаг по физике, ADR 0017): `relax-1s` вне допуска (опорная стопа
      проседает в мягкий контакт Box3D на 2–4 см под ударной нагрузкой); пол Box3D на 6.35 мм выше BSP; выстрел — M6.
      → M5 (ADR 0018): пол учтён; причина `relax-1s` уточнена — растяжение суставов, а не контакт.

**DoD:** стойка `bodyBalance` 60 с, толчок, `catchFall`, `relax`, первые 60 шагов выстрела — в допусках сценария;
`relax-1s` — вне допуска, причина и план в ADR 0017; 4 Homer одновременно — 67 тиков/с при цели 66.7 (~1 мс NM на
агента).

### M5. Мир для NM — 4–6 дней (параллельно с концом M4)

Сделано (ADR 0018). Отличия от плана: мир строится крейтом `delirium` (`Collision`, как у FOLLOW-агента M4), а не
C API; пол режется на квадраты 1 м, как пол Playroom; понадобились правки форка — материалы контактов NM
(`VBOX3D_NM_004`) и жёсткость суставов NM.

- [x] Статика: `.bsp` карты (Lua читает файл игры, `module/src/world.rs`): грани модели мира без
      `SKY/SKY2D/WARP/TRIGGER/NODRAW/HINT/SKIP`, displacement'ы, обход против часовой вокруг внешней нормали, без
      T-стыков; пол — квадратами 1 м по сетке кадра NM (как `world::floor_tris`) в радиусе 50 м. Материалы:
      `$surfaceprop` → строка materials.dat (`world::SURFACES`, неизвестное — CONCRETE). Кэш: карта — на сессию, мир —
      по началу кадра агента. Проверка `DMS_MapCheck`: высоты NM и трассы игры совпадают до 0.1 мм.
- [x] Динамика: пропы в 3 м от агента → `Obstacle` каждый тик (OBB, скорость, вращение, центр масс; `DMS.FollowObstacles`,
      телеметрия `obstacles`); транспорт — тем же путём (боксом по OBB).
- [x] Fallback: карта не читается — пол Playroom на высоте трассы вниз (`DMS_FloorBelow`).
- [x] Пол Box3D на 6.35 мм выше BSP: грани мира NM сдвинуты наружу на оболочку форка, пол под Homer — плоскость
      трассы (минус `DIST_EPSILON`) плюс оболочка; кадр NM каждого агента — на полу под ним.
- [x] Трение и упругость контактов как в физике NM: форк `VBOX3D_NM_004` (`SetBodyMaterial`, `SetSurfaceMaterial`):
      `μ_части × μ_строки materials.dat`, `0.4 × e`; суставы между телами NM — без «бюджета жёсткости» Box3D.
- [x] Слой комнаты Playroom на стадиях тем же кодом (ADR 0016): `stairs_frame` (`rollDownStairs`), `ledge` (rooftop).
- [ ] Вода: Homer в воде карты — как в бассейне Playroom (v0.2.1): то же сообщение `buoyancy` с теми же
      параметрами и поверхностью воды из карты; GPU-симуляцию воды Playroom не переносим.
- [x] Тестовая карта: `tools/dms-map` → `dms_stages.bsp` (стадии Playroom `bigstairs`, `stairsdown`, `rooftop`,
      `highledge`, `lowwall`, `kerb`, `rail`, `open` и наклон 20°, сайт на стадию); лёд — не сделан (с водой).
- [x] Golden-сценарии на стадиях (`#! window stage=…` → Playroom `--stage`, стенд `server.ps1 start -Map dms_stages`):
      `kerb`, `rooftop`, `lowwall`, `stairs`; проверка `said=` (реакции слоя комнаты). В допусках: `kerb`, все прежние
      сценарии кроме `relax-1s`, у `rooftop` — реакции и падение (кроме позы после приземления).
- [ ] Известные расхождения (ADR 0018): `relax-1s` (0.77 с против 1.00) — не мягкий контакт, а растяжение суставов
      Box3D под весом (голеностоп 6–10 мм; доп. проходы форка не включают контакты стоп); `lowwall`, `stairs` и
      приземление `rooftop` — хаотическое усиление того же расхождения стойки (2 см до толчка). План — контакты тел NM в
      доп. проходах Гаусса–Зейделя форка.
      → ADR 0019: контакты тел NM в доп. проходах сделаны (`box3d-armature-contacts.patch`): `stairs` в допуске,
      `lowwall` стоит (разрыв COM 0.12 м вместо 2.2), пики растяжения голеностопа 20 → 10 мм. Остаток — упругость
      мягкого сустава Box3D `F / (m_eff · ω²)` (~5 мм в стойке), из-за неё `relax-1s` (позже 1.00 с), поза `kerb`,
      `lowwall` (близко) и приземление `rooftop` пока вне допуска; план — жёсткость суставов NM выше потолка ¼.

**DoD:** golden-сценарии на стадиях Playroom (`--stage`: лестница, стена, уступ) совпадают с Playroom на той же
геометрии, собранной в GMod; контакты NM (`nm_agent_get_part_contact`) совпадают с касаниями в Box3D.

### M6. Lua API, поведения, реакции — 4–5 дней

Сделано (ADR 0020, [docs/api.md](docs/api.md)). Отличия от плана: слой реакций — порт методов `Sim` Playroom
(`module/src/follow/room.rs`), а не вызовы готовых функций (их в крейте нет); вставание «анимацией» — тела держит клип
(заморожены и ставятся в позу каждый тик), т.к. спрятать ragdoll, как Playroom прячет доллу, нельзя; Playroom для
эталонов пересобран (прежний `target-playroom` был старше исходников v0.2.2 и не знал `!miss`, `!set`), эталоны всех
сценариев сняты заново (`shot` и `catchfall` — байт в байт как прежние).

- [x] Публичный API (`dms.*`): `Attach(rag, opts)`, `Detach`, `ctrl:Send(msg, params)`, `ctrl:Shot`,
      `Balance`, `CatchFall`, `Flinch`, `Relax`, `Stop`, `GetState`, `GetParam`; `dms.Messages()` для интроспекции.
      Ещё `dms.Get/All/Reactions`, `ctrl:React/Button/Kill/SetHealth`; Homer из меню спавна привязывается при спавне.
- [x] Хуки: `DMS_Attached`, `DMS_Detached`, `DMS_Fallen`, `DMS_Feedback` (из очереди событий); ещё `DMS_Died`,
      `DMS_Reaction` (журнал слоя комнаты).
- [x] **Реакции — порт слоя реакций Playroom, а не свои** (ADR 0016, 0020): выстрел (`fire` + ShotTask, набор оружия
      `weapon_set`, дробь — `more`), удар (`land`: MeleeTask), взрыв (`blast_on` + explosion), огонь (`burn`: OnFire),
      шокер (`zap`), high fall (`in_air`), промах (`near_miss`), смерть, урон `ped::damage`/`bump::damage`, машина
      (`car_react`), проп (`obj_hit`), бросок (`throw`), доп. реакции (`self_preserve`). Импульс пули/взрыва — `applyImpulse`
      Playroom, сила урона GMod обнуляется. Golden: `shot` (600 шагов), `blast`, `nearmiss`, `shotgun`, `death`,
      `getup`, `tasks` в допусках. Исходный план: что Playroom делает на выстрел (ShotTask,
      наборы `taskparams` по оружию), удар (`meleetask`), взрыв (`explosion`), огонь (`onfire`), шокер, падение с
      высоты (high fall и приземление), промах рядом (near miss: flinch, баланс держит), смерть (dead Homer обмякает),
      здоровье и урон (`delirium-data` `ped::damage`, `fall_damage`, `car_hit`) — повторяется тем же кодом движка
      через крейт `delirium`. `EntityTakeDamage` по Homer → часть (`nm_agent_nearest_part`/хитгруппа) → та же
      задача Playroom; импульс пули в FOLLOW уже приложен движком — **не дублировать** `apply_push`.
- [x] Переключатели как в Playroom с его умолчаниями (`settings.txt` `--safe`): `tasks`, `avoid` (уклонение),
      дополнительные реакции v0.2.1 (руки на стену, поручни, упор в ящик, следит и пригибается от падающего, отклоняется
      от брошенного, оглядывается на ткнувшего), «дополнительные экспериментальные» v0.2.2 — каждая своим конваром,
      выключенные по умолчанию так же, как в Playroom. Сделано: конвары `dms_tasks look avoid getup extra_reactions
      extra_pick fire_reaction dive_shot dodge_gap weak god` с умолчаниями `--safe`; экспериментальное v0.2.2, что про тело
      Homer, — нырок под обстрелом (`dive_shot`); прочее (Hero, Skate 3) — `Отложено`.
- [x] **Вставание** как в Playroom (`getup`: 0 анимация, 1 мышцы по клипу, 2 выключено, 3 обученная сеть
      `getup.net`; триггер — здоровье ≥ 50, голова в 0.3 м от таза, тело неподвижно). Режим по умолчанию — как у
      Playroom. Клипы вставания — из копии игры (`Clips`, ADR 0010); режим «анимация» не NM — решить, как вести тело
      по клипу в FOLLOW (кинематика тел в форке или режим «мышцы» через `incomingTransforms`); `getup.net` — из
      Delirium (проверить лицензию/CREDITS перед распространением). Сделано: клипы `get_up_fast`/`idle` тем же кодом
      (`tools/dms-game/src/getup.rs`); «анимация» — тела держит клип, в конце новый агент + `bodyBalance` (ADR 0020);
      «мышцы» — `task_pose`; «обученная сеть» не перенесена (`learn.rs` — код комнаты): Homer лежит, как Playroom без
      `getup.net`. После вставания стойка из позы клипа на Box3D дрейфует и Homer переступает (Playroom стоит) — то же
      расхождение стойки, что в M5.
- [x] Консольные команды (суперадмин): `dms_attach`, `dms_send`, `dms_react`, `dms_detach_all`.
- [x] **Тюнинг и настройка поведений** (ADR 0009), серверная часть: профили `data/dms/profiles/*.json` (только
      отличия от умолчаний Playroom; профиль «Playroom» пустой и неизменяемый), переопределения параметров
      сообщений NM (`nm_msg_set_*`) и наборов `taskparams` (`<задача> <набор> <параметр>`), применение на
      следующем шаге, `dms_tuning 0|1`, активный профиль в `dms_status` и телеметрии; golden-прогоны — всегда без
      тюнинга. Сделано (`tuning.rs`, `tuning.lua`): значения профиля добавляются к строкам, которые запускают
      поведение; taskparams — копия файла игры (в PC-версии файла нет — нечего менять).
- [x] **Лицо Homer как в Playroom** (`delirium-room` `face_view`): клиентская отрисовка на капсуле головы — белки
      глаз, зрачки, брови; глаза моргают, смотрят на место удара или по скорости, щурятся от боли, закрываются без
      сознания, брови при испуге — по телеметрии агента (удар, скорость, боль, без сознания). Там же опциональный
      скелет-оверлей (`skeleton`, `bone_tint`) и выбор схемы цвета (скин модели). Сюда же из M4 — отладочный
      оверлей: капсулы NM поверх ragdoll'а, векторы drives, COM, stance.
- [x] Импульсы поведения `shot` (`shot_out.impulse`) и задач — на тела Box3D, как стабилизация и `applyImpulse` в M4.
      Импульсы задач (пуля, взрыв, удар, тычок) — `applyImpulse`, переносятся. `shot_out.impulse` (рука↔рана, пара с
      нулевой суммой) и `grab` лежат в закрытом `Ragdoll::pending`: без правки Delirium не достать — не переносятся
      (ADR 0020, п. 6).
- [x] **Меню тюнинга** в spawnmenu (вкладка «DMS», только суперадмин, `net`-сообщения на сервер):
      список поведений из `nm_message_count`/`nm_message_name`, для каждого — параметры с типом, описанием
      (`nm_message_param_doc`), значением по умолчанию и диапазоном: ползунки/флажки/векторы; параметры,
      которые порт принимает, но не использует (`nm_message_param_used` = 0), скрыты или помечены; реакции из
      таблицы реакций (какое поведение и с какими параметрами на выстрел/взрыв/огонь/падение); живое применение
      к выбранному Homer, «Сбросить к Playroom», сохранение/загрузка/удаление профилей. Сделано: «Утилиты → DMS»
      (`cl_menu.lua`); диапазонов параметров в API NM нет — из описания или вокруг умолчания.
- [ ] Визуальная проверка пользователем: лицо, скелет, оверлей, меню, вставание (docs/api.md, раздел «Клиент»).

**DoD:** все реакции из таблицы запускаются из консоли и Lua на Homer и отправляют NM те же сообщения с теми же
параметрами, что Playroom на ту же реакцию (golden-сценарии реакций); документация `docs/api.md`. В меню любой
параметр любого поведения меняется, сохраняется в профиль и сбрасывается к Playroom; с профилем «Playroom»
golden-сценарии проходят без изменений.

### M7. Надёжность, производительность, платформы — 5–7 дней (сквозной, финал перед релизом)

- [ ] Жизненный цикл: удаление ragdoll'а, `undo`, `gmod_admin_cleanup`, смена карты, `lua_refresh`, выгрузка
      модуля, сохранение/загрузка (агенты не сохраняются — пересоздаются), дубликатор, засыпание объектов,
      заморозка физганом, сварка/верёвки на частях, телепорт (`SetPos`) — сброс агента.
- [ ] Listen server: два окружения в одном процессе — хукать только серверное (`GetObjectEnvironment`).
- [ ] Производительность: бюджет на агента (цель — ≤ 0.15 мс/шаг), лимит агентов (`dms_max_agents`),
      LOD: дальние/спящие агенты переводятся в `relax` и отцепляются.
- [ ] Linux: сборка `gmsv_dms_linux64.so` / `linux.so` в Steam Runtime (как сборка форка), проверка на
      dedicated-сервере. Требует порта форка на серверный ABI GMod x64 (ADR 0012) — решить, нужен ли dedicated
      вообще; тесты при этом остаются в игре.
- [ ] Краш-устойчивость: никаких паник через FFI-границу (`catch_unwind` в каждом `extern` входе).

**DoD:** 30-минутный прогон «бот-сервер, 8 агентов, спавн/удаление/смена карты» без падений и утечек; Linux
dedicated работает.

### M8. Релиз — 2–3 дня

- [ ] Конвары и меню в spawnmenu (утилиты: привязать, реакция, отладка) вместе с меню тюнинга из M6 — одна
      вкладка «DMS», полировка UI и подсказок.
- [ ] Документация: установка (форк `vphysics.dll` + бэкап, бинарный модуль в `lua/bin`, данные GTA IV в
      `data/dms`), FAQ («Steam при проверке файлов вернёт оригинальный vphysics»), API.
- [ ] Дистрибуция: Lua-часть — Workshop; модель Homer не распространяется, её делает `dms-homer` у пользователя; `vphysics.dll` и `gmsv_dms_*.dll` Workshop не принимает —
      GitHub Releases + `install.ps1`.
- [ ] Версионирование и проверка совместимости при старте: версия модуля ↔ версия Lua-аддона ↔ интерфейс NM.

### Отложено: оригинальные модели GMod (ValveBiped)

Пока не делаем — работаем только с телом Homer. Когда понадобятся модели, для них пишется **новый инструмент с
новым подходом** (проектируется тогда же, отдельной вехой); инструмент прошлой попытки (`delirium-ragdoll`)
игнорируется. Сюда же относится авто-привязка ragdoll'ов смерти NPC/игроков.

### Отложено: возможности Playroom, не нужные для поведения Homer

Не переносим (это приложение Playroom, а не поведение тела): Hero mode, оружейная панель и плавающий пистолет, Portal
и Skate 3, обученная ходьба/бег (`walk.net`/`run.net`), GPU-вода, огонь/кровь/обугливание/слои урона как графика,
графические режимы, толпы (`crowd`/`grid`). Кандидаты на потом: пресеты тела (`--body S|A|L|K|F|T`, Homer через
`Build::scaled` — дёшево, но это уже не 1.8 м), несколько Homer в одной сцене как в «Spawn Homers».

---

## Зависимости и параллельность

```
M0 ──► M1 ─┐
   └─► M2 ─┴─► M2.5 ──► M3 ──► M4 ──► M6 ──► M8
                                └───► M5 ─┘    ▲
                       M7 (сквозной, закрывается перед M8) ┘
```

Критический путь: **M0 → M1/M2 → M2.5 → M3 → M4**. С M2.5 все проверки в игре, кроме визуальных, Claude делает
сам на стенде в игре (без окна).

## Риски

| Риск | Влияние | Митигация |
|---|---|---|
| 66 Гц Source vs 60 Гц NM | дрожь, неверные drives | проверить в M4 на эталоне; аккумулятор шагов |
| Гравитация 15.24 (Source) vs 9.8 (NM) | тело оседает и падает | компенсирующая сила на каждую часть, `sv_gravity` не меняется (ADR 0007, M4) |
| Контакты NM (по своему миру) расходятся с Box3D | баланс «не видит» опору | точный BSP-меш (M5), боксы для пропов; в крайнем случае — расширение форка с контактами частей |
| Порядок vtable `IPhysicsNM` | падение при вызове | FFI-тесты M1 |
| C API SDK не отдаёт того, что делает Playroom (outbox, задачи движка, реакции комнаты) | catchFall, выстрел, взрыв не 1 в 1 | тот же код через крейт `delirium`, как `delirium-room` (ADR 0016, M4/M6) |
| Новые версии Delirium меняют поведение (v0.2.2: стойка, catchFall) | эталоны устаревают | при каждом обновлении SDK — новые эталоны Playroom и полная golden-сверка (как для v0.2.2) |
| Стенд занимает игру пользователя (одна копия GMod) | пользователь не может играть во время прогонов | `server.ps1 stop` сразу после проверки; длинные прогоны — когда пользователь не играет |
| Выделенный сервер GMod x64 несовместим с форком (новый ABI vphysics) | нет dedicated/Linux | тесты только в игре (решение M2.5); порт форка — только если понадобится в M7 |
| Канал `dms-ctl` открыт наружу | удалённое управление сервером | только `127.0.0.1`, только при `dms_dev 1`, пароль на запуск, в релизе выключен (M2.5, M8) |
| Физика Box3D отличается от физики Playroom (инерция, солвер суставов, контакты) | поведение не 1 в 1 | golden-сверка тела (M3) и поведения (M4); правки в форке, не в NM (ADR 0008) |
| `dms-homer` портирует тело неточно | все последующие вехи меряют не то | `tools/golden body` против `NM_DUMP` Playroom, допуск 1e-5 (M3) |
| PC `Fred.art` little-endian | мусор в суставах, NaN | `delirium_data::LE` перед чтением, как в Playroom; сверка `golden dump/lead` (M2, ADR 0010) |
| Steam откатывает `vphysics.dll` | аддон «молча» не работает | явная проверка интерфейса при старте и сообщение игроку (M1) |
| Несовместимые изменения `IPhysicsNM` в форке | падения | строгая проверка строки версии, копия заголовка в репо |
| Лицензия GPL-3 и данные GTA IV | юридические | GPL-3 для всего, данные только из копии пользователя |

## Ближайшие шаги (первая неделя)

1. M0 целиком: git, ADR, коммит NM-расширения в форке, сборочные скрипты, «hello» модуль.
2. M1: FFI `IPhysicsNM` + `PhysObj → IPhysicsObject*` + тест «висящий проп».
3. M2 параллельно: загрузка данных и `dms_selftest`.
