# DMS in Gmod — Roadmap

Delirium (active ragdoll, NM-style) нативно в Garry's Mod в режиме **FOLLOW**: тело — это обычный `prop_ragdoll`
на физике VPhysics-Box3D, Delirium каждый физический шаг читает состояние частей и возвращает силы/моменты, а
армирование суставов (armature) решается внутри солвера Box3D.

## Исходные данные

| Компонент | Где | Что даёт | Правило |
|---|---|---|---|
| Delirium SDK | `../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)/crates/delirium-sdk` | C API `delirium_physics.h` (`nm_*`), Rust-крейт `delirium_physics` (`rlib` + `cdylib`) | **только чтение**, подключается path-зависимостью |
| VPhysics-Box3D (форк) | `vphysics/` (git subtree, ADR 0015) | `vphysics.dll` + `IPhysicsNM` (`VBOX3D_NM_003`, `vphysics/include/vphysics_nm.h`) | менять только ради совпадения с Playroom (ADR 0008), upstream — `git subtree pull` ([docs/vphysics.md](docs/vphysics.md)) |
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

### M4. FOLLOW-цикл на эталоне — 5–8 дней (главная веха)

- [ ] `step.rs`: на каждый шаг для каждого агента — применить очередь команд → `GetBodyState` ×21 →
      преобразование в кадр капсулы (COM → центр капсулы через `GetMassCenterLocalSpace`) →
      `nm_agent_set_part_states` → `nm_agent_step(env, dt)` → `ApplyBodyDrive` ×21 → `SetJointArmature` ×20.
- [ ] Время (ADR 0008): NM получает тот же шаг, что в Playroom (1/60 с); 66 тиков Source сводятся к нему на нашей
      стороне (аккумулятор времени, drives держатся до следующего шага NM) — сравнить с golden и с `-tickrate 60`.
- [ ] Гравитация (ADR 0007): `sv_gravity` не трогаем. NM считает с 9.8 м/с², Source — 15.24 м/с²; на каждую часть
      вместе с drives добавляется `F_k = m_k · (g_nm − g_src)` (≈ 5.44·m вверх), `g_src` из `physenv.GetGravity()`
      через очередь команд. Режимы `dms_gravity_mode nm | hybrid | off`: замерить высоту таза, устойчивость стойки
      и время падения против Playroom, выбрать режим по умолчанию.
- [ ] Защита: NaN/Inf в состоянии или drives → агент снимается, armature сбрасывается, ragdoll остаётся обычным.
- [ ] Отладка: клиентский оверлей (капсулы NM поверх ragdoll'а, векторы сил, COM, stance), `dms_status`,
      CSV-лог высоты таза/энергии/флагов `fallen/balancing` для сравнения прогонов.
- [ ] Golden-сверка поведения (ADR 0008): `dms_script <сценарий>` в GMod выполняет те же сценарии, что Playroom, и
      пишет запись в формате `--rec`; `tools/golden behaviour` сравнивает таз/COM/касания/флаги по времени с
      `golden/out/`. Расхождения разбираются как разница входных данных или физики (форк), не тюнингом NM.

**DoD:** все golden-сценарии на ровном полу совпадают с Playroom в допусках сценария (стойка `bodyBalance` 60 с,
выстрел в грудь, толчок, `catchFall`, `relax`); ≥ 4 Homer одновременно без просадки тикрейта.

### M5. Мир для NM — 4–6 дней (параллельно с концом M4)

- [ ] Статика: чтение `.bsp` карты (из файловой системы игры): брашевые грани без `NODRAW/SKY/trigger`,
      displacement'ы → `nm_world_add_mesh`, материалы поверхностей → байт материала → `nm_set_surface`
      (трение льда, грязи и т.п.). Кэш меша на карту.
- [ ] Динамика: пропы рядом с агентами (AABB-запрос) → `nm_world_set_boxes` каждый шаг (центр, поворот,
      полуразмеры, скорость); транспорт — тоже боксами.
- [ ] Fallback: если карта не читается — пол на высоте трассировки вниз (`nm_world_add_floor`).
- [ ] Тестовая карта/сцена: лестница, уступ, стена, наклон, лёд.

**DoD:** golden-сценарии на стадиях Playroom (`--stage`: лестница, стена, уступ) совпадают с Playroom на той же
геометрии, собранной в GMod; контакты NM (`nm_agent_get_part_contact`) совпадают с касаниями в Box3D.

### M6. Lua API, поведения, реакции — 4–5 дней

- [ ] Публичный API (`dms.*`): `Attach(rag, opts)`, `Detach`, `ctrl:Send(msg, params)`, `ctrl:Shot`,
      `Balance`, `CatchFall`, `Flinch`, `Relax`, `Stop`, `GetState`, `GetParam`; `dms.Messages()` для интроспекции.
- [ ] Хуки: `DMS_Attached`, `DMS_Detached`, `DMS_Fallen`, `DMS_Feedback` (из очереди событий).
- [ ] Урон: `EntityTakeDamage` по ragdoll'у → часть по `nm_agent_nearest_part`/хитгруппе → `nm_agent_task_shot`.
      Импульс пули в FOLLOW уже приложен движком — **не дублировать** `apply_push`. Взрыв/огонь/шок/падение →
      соответствующие сообщения; таблица реакций в Lua, переопределяемая.
- [ ] Консольные команды (суперадмин): `dms_attach`, `dms_send`, `dms_react`, `dms_detach_all`.
- [ ] **Тюнинг и настройка поведений** (ADR 0009), серверная часть: профили `data/dms/profiles/*.json` (только
      отличия от умолчаний Playroom; профиль «Playroom» пустой и неизменяемый), переопределения параметров
      сообщений NM (`nm_msg_set_*`) и наборов `taskparams` (`<задача> <набор> <параметр>`), применение на
      следующем шаге, `dms_tuning 0|1`, активный профиль в `dms_status` и телеметрии; golden-прогоны — всегда без
      тюнинга.
- [ ] **Лицо Homer как в Playroom** (`delirium-room` `face_view`): клиентская отрисовка на капсуле головы — белки
      глаз, зрачки, брови; глаза моргают, смотрят на место удара или по скорости, щурятся от боли, закрываются без
      сознания, брови при испуге — по телеметрии агента (удар, скорость, боль, без сознания). Там же опциональный
      скелет-оверлей (`skeleton`, `bone_tint`) и выбор схемы цвета (скин модели).
- [ ] **Меню тюнинга** в spawnmenu (вкладка «DMS», только суперадмин, `net`-сообщения на сервер):
      список поведений из `nm_message_count`/`nm_message_name`, для каждого — параметры с типом, описанием
      (`nm_message_param_doc`), значением по умолчанию и диапазоном: ползунки/флажки/векторы; параметры,
      которые порт принимает, но не использует (`nm_message_param_used` = 0), скрыты или помечены; реакции из
      таблицы реакций (какое поведение и с какими параметрами на выстрел/взрыв/огонь/падение); живое применение
      к выбранному Homer, «Сбросить к Playroom», сохранение/загрузка/удаление профилей.

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
