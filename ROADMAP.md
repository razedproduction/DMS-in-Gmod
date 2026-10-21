# DMS in Gmod — Roadmap

Delirium (active ragdoll, NM-style) нативно в Garry's Mod в режиме **FOLLOW**: тело — это обычный `prop_ragdoll`
на физике VPhysics-Box3D, Delirium каждый физический шаг читает состояние частей и возвращает силы/моменты, а
армирование суставов (armature) решается внутри солвера Box3D.

## Исходные данные

| Компонент | Где | Что даёт | Правило |
|---|---|---|---|
| Delirium SDK | `../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)/crates/delirium-sdk` | C API `delirium_physics.h` (`nm_*`), Rust-крейт `delirium_physics` (`rlib` + `cdylib`) | **только чтение**, подключается path-зависимостью |
| VPhysics-Box3D (форк) | `../VPhysics-Box3D` | `vphysics.dll` + `IPhysicsNM` (`VBOX3D_NM_002`, `include/vphysics_nm.h`) | NM-расширение пока **не закоммичено** |
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
  docs/              adr/, data-setup.md, api.md
  scripts/           build.ps1, install.ps1 (копирование dll/аддона в GMod)
```

---

## Этапы

Оценки — ориентировочные, в «рабочих днях одного человека». Каждый этап закрывается только по чек-листу DoD.

### M0. Фундамент — 1–2 дня

- [ ] `git init`, `.gitignore`, LICENSE (GPL-3.0), этот roadmap.
- [ ] ADR 1–6 (выше) записаны.
- [ ] В форке VPhysics-Box3D закоммитить NM-расширение (сейчас `physics_nm.*`, `include/`, `cmake/patches/` не
      в git) отдельной веткой, собрать `windows-x64` (поддерживается только x86-64 ветка GMod).
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

### M3. Модель Homer: импорт с нуля под NM — 6–9 дней

Цель: ragdoll Homer, у которого физика совпадает с телом NM, чтобы любые проблемы FOLLOW были проблемами
интеграции, а не подгонки. Дальше все вехи работают только с ним (дальше «эталон» = Homer).

Модель **импортируется заново**, ничего не берётся из прошлой попытки (её `delirium-ragdoll`, `homer.mdl` и
`.nmfit.txt` не используются). Принцип — максимальная совместимость с NM: всё, что касается тела, берётся из тех же
данных и тем же кодом, каким Delirium строит Homer сам (`delirium-data` + `delirium::ragdoll::Rig`, как
`getup::homer` в Playroom), без подгонок (`nm_build_fit`) и ручных правок.

- [ ] `tools/dms-homer` (Rust CLI, path-зависимости на `delirium`/`delirium-data`, только чтение): читает из копии
      GTA IV пользователя фрагмент педа Homer (`componentpeds.img/<model>.xft`: скелет, части ragdoll'а, меш) и
      `fred.xrt`/`character.json`, строит `Build` тем же `Rig`, что и Delirium, и сверяет его с `nm_build_part_info`/
      `nm_agent_get_joint` — расхождение = ошибка импорта.
- [ ] Физика — один в один с NM: 21 физобъект в порядке частей NM (`nm_part_name`), кадр физкости = кадр капсулы NM
      (центр капсулы, ось Y), капсулы точного радиуса/полудлины (выпуклая оболочка с достаточным числом сегментов),
      массы NM (`$mass` по частям, центр масс в центре капсулы), суставы ball/hinge как `kind`, пивоты и лимиты из
      `nm_agent_get_joint`, `$jointcollide` только по self-collision парам NM, демпфирование в `.phy` — нулевое
      (демпфирует NM).
- [ ] Визуал: меш Homer из того же `.xft`, перескиненный на 21 кость NM (веса костей GTA → их части NM); если меш не
      читается — только капсулы. Скелет анимаций GMod не нужен (модель только `prop_ragdoll`).
- [ ] Сборка: SMD (reference + physics) + `.qc` → `studiomdl` GMod → `garrysmod/models/dms/homer.mdl` + `.phy`;
      материалы/текстуры — из того же `.xft` в `materials/models/dms/homer/`.
- [ ] Модель генерируется на машине пользователя из его копии игры и **не распространяется** (ADR 0006).
- [ ] Проверка в игре: `vcollide_wireframe 1`, массы (`PhysObj:GetMass`), число физобъектов = 21,
      все 20 constraint'ов находятся через `FindConstraint(parent, child)`.
- [ ] Bind (`bind.rs`): в позе покоя — `SetJointAnchor` (pivot NM) и `SetJointLimits` (frame + limits, hinge/ball
      по `kind`) на каждом суставе; таблица «часть NM → индекс физобъекта» по именам костей.
- [ ] Спавн: `dms_homer` ставит Homer в позе build'а перед игроком.
- [ ] **Проверка точности порта `dms-homer` против Playroom** (`tools/golden body`):
      - build из `dms-homer` против `NM_DUMP` Playroom (`m_y_pmanhat_01` после `getup::homer` + `stand_up`): имена и
        порядок частей, родители, массы, радиусы, полудлины, кадры стоячей позы, пивоты, рамки, лимиты и тип
        суставов, пары self-collision — допуск 1e-5;
      - скомпилированная модель: `.phy` (массы, центры масс, оболочки капсул — отклонение объёма/радиуса),
        `$collisionjoints` против build'а;
      - в игре: позы 21 физобъекта заспавненного Homer против кадров `NM_DUMP`, массы и тензоры инерции Box3D
        против инерции капсул Delirium; расхождение инерции — правка в форке (ADR 0008), не в NM;
      - отчёт по каждой части/суставу с максимальными ошибками.

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

**DoD:** все реакции из таблицы запускаются из консоли и Lua на Homer и отправляют NM те же сообщения с теми же
параметрами, что Playroom на ту же реакцию (golden-сценарии реакций); документация `docs/api.md`.

### M7. Надёжность, производительность, платформы — 5–7 дней (сквозной, финал перед релизом)

- [ ] Жизненный цикл: удаление ragdoll'а, `undo`, `gmod_admin_cleanup`, смена карты, `lua_refresh`, выгрузка
      модуля, сохранение/загрузка (агенты не сохраняются — пересоздаются), дубликатор, засыпание объектов,
      заморозка физганом, сварка/верёвки на частях, телепорт (`SetPos`) — сброс агента.
- [ ] Listen server: два окружения в одном процессе — хукать только серверное (`GetObjectEnvironment`).
- [ ] Производительность: бюджет на агента (цель — ≤ 0.15 мс/шаг), лимит агентов (`dms_max_agents`),
      LOD: дальние/спящие агенты переводятся в `relax` и отцепляются.
- [ ] Linux: сборка `gmsv_dms_linux64.so` / `linux.so` в Steam Runtime (как сборка форка), проверка на
      dedicated-сервере.
- [ ] Краш-устойчивость: никаких паник через FFI-границу (`catch_unwind` в каждом `extern` входе).

**DoD:** 30-минутный прогон «бот-сервер, 8 агентов, спавн/удаление/смена карты» без падений и утечек; Linux
dedicated работает.

### M8. Релиз — 2–3 дня

- [ ] Конвары и меню в spawnmenu (утилиты: привязать, реакция, отладка).
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
   └─► M2 ─┴─► M3 ──► M4 ──► M6 ──► M8
                       └───► M5 ─┘    ▲
              M7 (сквозной, закрывается перед M8) ┘
```

Критический путь: **M0 → M1/M2 → M3 → M4**.

## Риски

| Риск | Влияние | Митигация |
|---|---|---|
| 66 Гц Source vs 60 Гц NM | дрожь, неверные drives | проверить в M4 на эталоне; аккумулятор шагов |
| Гравитация 15.24 (Source) vs 9.8 (NM) | тело оседает и падает | компенсирующая сила на каждую часть, `sv_gravity` не меняется (ADR 0007, M4) |
| Контакты NM (по своему миру) расходятся с Box3D | баланс «не видит» опору | точный BSP-меш (M5), боксы для пропов; в крайнем случае — расширение форка с контактами частей |
| Порядок vtable `IPhysicsNM` | падение при вызове | FFI-тесты M1 |
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
