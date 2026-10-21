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
4. **Эталонное тело 1:1 первым.** Сначала модель с 21 костью = 21 физобъект, кадр кости = кадр капсулы NM
   (нулевые смещения). Только после стабильного FOLLOW на нём — реальные модели.
5. **Нет `IPhysicsNM` → аддон выключается** с понятным сообщением (никакого fallback на ванильную физику в FOLLOW).
6. Лицензия **GPL-3.0-only** (как у SDK). Игровые данные GTA IV не распространяются.

Структура репозитория:

```
DMS in Gmod/
  module/            gmsv_dms (Rust): ffi/vphysics_nm.rs, ffi/lua.rs, agent.rs, world.rs, bind.rs, step.rs
  tools/             dms-ragdoll (Rust CLI): генерация моделей/.phy под тело NM
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
      (Linux: `dlopen(..., RTLD_NOLOAD)`, `vphysics_srv.so` на dedicated — в M8). Если `NULL` — пробуем `_001`
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

- [ ] Path-зависимость на `delirium-sdk`, вызовы SDK как Rust-функций (обёртки с проверкой кодов `NM_E_*`
      и `nm_last_error`).
- [ ] Загрузка данных из `garrysmod/data/dms/`: `character.json` (предпочтительно) или `fred.xrt`, `build.txt`,
      `taskparams.txt`. Конвары путей. Для PC-версии `Fred.art` (little-endian) — проверка порядка байт
      или требование `character.json`.
- [ ] Утилита (`tools/`, cargo example) «сделать `character.json` из файлов игры» + `docs/data-setup.md`.
- [ ] Офлайн-самопроверка внутри модуля: агент в LEAD на плоском полу, 2 с `bodyBalance` — стоит
      (команда `dms_selftest`).
- [ ] Справочные нативы: имена частей, список сообщений/параметров, `nm_build_part_info`, `nm_agent_get_joint`.

**DoD:** `dms_selftest` проходит в игре; без данных — внятная ошибка с путём, куда положить файлы.

### M3. Эталонное тело 1:1 — 4–6 дней

Цель: ragdoll, у которого физика совпадает с телом NM, чтобы любые проблемы FOLLOW были проблемами интеграции,
а не подгонки.

- [ ] `tools/dms-ragdoll --reference`: из `build` (+ `nm_agent_get_joint`) генерирует SMD-скелет из 21 кости
      (кадр кости = кадр капсулы NM), коллизию-капсулы (выпуклые оболочки), `.qc` с `$collisionjoints`:
      массы NM, лимиты суставов, `$jointcollide`-пары по self-collision NM; компиляция через `studiomdl` GMod.
- [ ] Проверка в игре: `vcollide_wireframe 1`, массы (`PhysObj:GetMass`), число физобъектов = 21,
      все 20 constraint'ов находятся через `FindConstraint(parent, child)`.
- [ ] Bind (`bind.rs`): в позе покоя — `SetJointAnchor` (pivot NM) и `SetJointLimits` (frame + limits, hinge/ball
      по `kind`) на каждом суставе; таблица «часть NM → индекс физобъекта» по именам костей.
- [ ] Спавн: `dms_spawn_reference` ставит модель в позе build'а перед игроком.

**DoD:** эталон, брошенный без NM, ведёт себя как обычный ragdoll, суставы гнутся в пределах NM, ничего не
взрывается; лимиты видны через `IPhysicsConstraint::GetConstraintTransform`.

### M4. FOLLOW-цикл на эталоне — 5–8 дней (главная веха)

- [ ] `step.rs`: на каждый шаг для каждого агента — применить очередь команд → `GetBodyState` ×21 →
      преобразование в кадр капсулы (COM → центр капсулы через `GetMassCenterLocalSpace`) →
      `nm_agent_set_part_states` → `nm_agent_step(env, dt)` → `ApplyBodyDrive` ×21 → `SetJointArmature` ×20.
- [ ] Время: `dt` из callback'а (1/66) — проверить, что NM корректно принимает не-1/60; если нет — аккумулятор
      времени с шагом NM и удержанием последних drives.
- [ ] Гравитация: NM считает с 9.81 м/с², Source по умолчанию 600 u/s² ≈ 15.24 м/с². Решение (ADR): либо
      требовать `sv_gravity 386`, либо компенсировать разницу силой на каждую часть. Проверить оба.
- [ ] Защита: NaN/Inf в состоянии или drives → агент снимается, armature сбрасывается, ragdoll остаётся обычным.
- [ ] Отладка: клиентский оверлей (капсулы NM поверх ragdoll'а, векторы сил, COM, stance), `dms_status`,
      CSV-лог высоты таза/энергии/флагов `fallen/balancing` для сравнения прогонов.
- [ ] Метрики сравнения с эталонами SDK (`mujoco_follow.py`): «стоит 1 с», «после удара в грудь 40 Н·с —
      отшатывается/падает».

**DoD:** на ровном полу эталон под `bodyBalance` стоит ≥ 60 с без дрожи; от толчка физганом — шаги/падение;
`catchFall` при падении; `relax` — обмякает; ≥ 4 эталона одновременно без просадки тикрейта.

### M5. Мир для NM — 4–6 дней (параллельно с концом M4)

- [ ] Статика: чтение `.bsp` карты (из файловой системы игры): брашевые грани без `NODRAW/SKY/trigger`,
      displacement'ы → `nm_world_add_mesh`, материалы поверхностей → байт материала → `nm_set_surface`
      (трение льда, грязи и т.п.). Кэш меша на карту.
- [ ] Динамика: пропы рядом с агентами (AABB-запрос) → `nm_world_set_boxes` каждый шаг (центр, поворот,
      полуразмеры, скорость); транспорт — тоже боксами.
- [ ] Fallback: если карта не читается — пол на высоте трассировки вниз (`nm_world_add_floor`).
- [ ] Тестовая карта/сцена: лестница, уступ, стена, наклон, лёд.

**DoD:** эталон поднимается/спускается по лестнице под толчками, `teeter` на уступе, `brace` при наезде машины;
контакты NM (`nm_agent_get_part_contact`) совпадают с визуальными касаниями.

### M6. Lua API, поведения, реакции — 4–5 дней

- [ ] Публичный API (`dms.*`): `Attach(rag, opts)`, `Detach`, `ctrl:Send(msg, params)`, `ctrl:Shot`,
      `Balance`, `CatchFall`, `Flinch`, `Relax`, `Stop`, `GetState`, `GetParam`; `dms.Messages()` для интроспекции.
- [ ] Хуки: `DMS_Attached`, `DMS_Detached`, `DMS_Fallen`, `DMS_Feedback` (из очереди событий).
- [ ] Урон: `EntityTakeDamage` по ragdoll'у → часть по `nm_agent_nearest_part`/хитгруппе → `nm_agent_task_shot`.
      Импульс пули в FOLLOW уже приложен движком — **не дублировать** `apply_push`. Взрыв/огонь/шок/падение →
      соответствующие сообщения; таблица реакций в Lua, переопределяемая.
- [ ] Консольные команды (суперадмин): `dms_attach`, `dms_send`, `dms_react`, `dms_detach_all`.

**DoD:** все реакции из таблицы запускаются из консоли и Lua на эталоне, документация `docs/api.md`.

### M7. Реальные модели (ValveBiped) — 7–12 дней

Главный выбор этапа — как получить 21 сустав на моделях с 15 физобъектами:

- **(A, основной) конвертация модели:** `tools/dms-ragdoll --convert <model.mdl>` — подогнать build NM к костям
  модели (`nm_build_fit`), сгенерировать новую `.phy` с 21 объектом (позвоночник, ключицы, шея добавлены),
  перенести веса вершин (reskin) — модель кладётся в `models/dms/...` и подменяет ragdoll при привязке.
- **(B, запасной) runtime-сведение:** несколько частей NM на один физобъект (позвоночник на таз/грудь,
  ключицы на грудь, шея на голову): drives частей суммируются (сила + момент относительно COM объекта),
  armature — только на реальных суставах. Качество ниже, но работает с любой моделью «как есть».

- [ ] Прототип B на `models/humans/group01/male_07.mdl` (быстро, проверяет переносимость FOLLOW).
- [ ] Конвейер A на 3–5 моделях HL2 (citizen, combine, police), сравнение A vs B по метрикам M4.
- [ ] `dms.RegisterSkeleton(model, map)` для нестандартных скелетов.
- [ ] Авто-привязка: ragdoll'ы смерти NPC/игроков (`CreateEntityRagdoll`, `OnEntityCreated`), конвар
      `dms_autoattach`, «умирающий» сценарий: `bodyBalance` → `catchFall` → `relax`.

**DoD:** убитый NPC-ситизен реагирует на попадания, пытается устоять, падает с catchFall — на стандартных
моделях HL2 с конвертацией (A) и без неё (B).

### M8. Надёжность, производительность, платформы — 5–7 дней (сквозной, финал перед релизом)

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

### M9. Релиз — 2–3 дня

- [ ] Конвары и меню в spawnmenu (утилиты: привязать, реакция, отладка).
- [ ] Документация: установка (форк `vphysics.dll` + бэкап, бинарный модуль в `lua/bin`, данные GTA IV в
      `data/dms`), FAQ («Steam при проверке файлов вернёт оригинальный vphysics»), API.
- [ ] Дистрибуция: Lua-часть и модели — Workshop; `vphysics.dll` и `gmsv_dms_*.dll` Workshop не принимает —
      GitHub Releases + `install.ps1`.
- [ ] Версионирование и проверка совместимости при старте: версия модуля ↔ версия Lua-аддона ↔ интерфейс NM.

---

## Зависимости и параллельность

```
M0 ──► M1 ─┐
   └─► M2 ─┴─► M3 ──► M4 ──► M6 ──► M7 ──► M9
                       └───► M5 ─┘           ▲
              M8 (сквозной, закрывается перед M9) ┘
```

Критический путь: **M0 → M1/M2 → M3 → M4**. Пока M4 не стоит на эталоне, к реальным моделям (M7) не переходить.

## Риски

| Риск | Влияние | Митигация |
|---|---|---|
| 66 Гц Source vs 60 Гц NM | дрожь, неверные drives | проверить в M4 на эталоне; аккумулятор шагов |
| Гравитация 600 vs 9.81 | тело оседает/подпрыгивает | ADR в M4: `sv_gravity 386` или компенсация |
| Контакты NM (по своему миру) расходятся с Box3D | баланс «не видит» опору | точный BSP-меш (M5), боксы для пропов; в крайнем случае — расширение форка с контактами частей |
| 15 физобъектов vs 21 часть | плохой FOLLOW на стандартных моделях | конвертация моделей (M7-A) как основной путь |
| x86 thiscall / порядок vtable | падение при вызове | FFI-тесты M1 на обеих архитектурах |
| PC `Fred.art` little-endian | мусор в суставах, NaN | `character.json` как основной формат (M2) |
| Steam откатывает `vphysics.dll` | аддон «молча» не работает | явная проверка интерфейса при старте и сообщение игроку (M1) |
| Несовместимые изменения `IPhysicsNM` в форке | падения | строгая проверка строки версии, копия заголовка в репо |
| Лицензия GPL-3 и данные GTA IV | юридические | GPL-3 для всего, данные только из копии пользователя |

## Ближайшие шаги (первая неделя)

1. M0 целиком: git, ADR, коммит NM-расширения в форке, сборочные скрипты, «hello» модуль.
2. M1: FFI `IPhysicsNM` + `PhysObj → IPhysicsObject*` + тест «висящий проп».
3. M2 параллельно: загрузка данных и `dms_selftest`.
