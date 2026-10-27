# Стенд: GMod + `dms-ctl` (ROADMAP M2.5)

Стенд — та же роль, что у Playroom `--headless`/`--script --rec`, но на Source + Box3D: Claude сам запускает
Garry's Mod, управляет им и в реальном времени видит состояние; пользователю остаётся визуальная проверка.
Решения — [ADR 0012](adr/0012-bench-game-and-dms-ctl.md).

## Одной командой

```powershell
.\scripts\build.ps1                     # модуль -> dist\
cargo build --release -p dms-ctl -p golden
.\scripts\server.ps1 start              # игра в окне, канал dms-ctl и RCON на 127.0.0.1
.\target\release\dms-ctl.exe exec status
.\target\release\dms-ctl.exe lua "return DMS.Version"
.\target\release\dms-ctl.exe watch --for 2s
.\target\release\dms-ctl.exe run golden\scenarios\stand-1s.txt
.\target\release\golden.exe behaviour --source
.\scripts\server.ps1 stop
```

## `scripts\server.ps1`

- `start` — ставит `dist\` и `addon\` в игру (`install.ps1`), пишет `garrysmod\data\dms\bench.txt` (`dms_game_dir`
  из `golden\game_dir.txt`/`DMS_GAME_DIR`/`-GtaDir`, `dms_dev 1`, новый токен) и запускает **саму игру**:
  `gmod_win64.exe -w 1280 -h 720 -windowed -nosound -novid -usercon -condebug -ip 127.0.0.1 -port 27115 +maxplayers 2
  +sv_lan 1 +sv_cheats 1 +sv_hibernate_think 1 +rcon_password <новый> +map gm_flatgrass`. Окно видно, чтобы за тестом
  можно было смотреть; тики идут 66/с и без фокуса, и свёрнутым. `start -Hidden` — без отрисовки (`-textmode`,
  640×480), окно скрывается (`ShowWindow`), как было в M2.5.
  Это listen-сервер: `maxplayers 2`, иначе одиночная игра без RCON; `-usercon` включает RCON. Хост — аккаунт Steam
  пользователя на loopback. Аддон читает `bench.txt` и сразу удаляет его, так что обычная игра канал не открывает.
  Адреса и секреты — в `.bench\server.txt` (не в git), их читает `dms-ctl`. Лог консоли —
  `garrysmod\console.log`.
- `stop` — `quit` через RCON, через 10 с — принудительно; `restart`, `status`.
- Игра не запускается дважды: пока стенд работает, пользователь не играет (и наоборот) — `start` проверяет это.
- `-Dedicated [-Server <dir>]` — то же на выделенном сервере (`srcds_win64.exe`, SteamCMD app 4020, ветка x86-64;
  `update` ставит/обновляет его). **Форк VPhysics-Box3D там не работает**: серверная сборка GMod x64 собрана с
  более новыми интерфейсами (`IAppSystem` с `GetDependencies/GetTier/Reconnect/IsSingleton`: 17 слотов vtable
  `VPhysics031` против 13 у клиента и форка; `VPhysicsCollision007` — 59 против 52; `VPhysicsSurfaceProps001` тоже
  длиннее), и форк роняет `dedicated.dll`/`server.dll`. Поэтому сервер оставляет свой `vphysics.dll`: канал,
  тестовый агент и записи работают, `IPhysicsNM` нет. Порт форка на серверный ABI — отдельная задача.

## Канал модуля (`module/src/dev.rs`, `addon/lua/dms/dev.lua`)

Только при `dms_dev 1` (по умолчанию 0), только `127.0.0.1:dms_dev_port` (27099), только с `dms_dev_token`
(первый запрос — `auth`). Сетевые потоки ни Lua, ни физику не трогают (ADR 0003): кладут запросы в очередь и
отдают готовые кадры. Очередь разбирает `Think` (`DMS.DevThink`): консольные команды и Lua возвращаются в Lua,
сообщения NM и сценарии идут тестовому агенту. Кадр: строка `<id> <вид> <длина>` и `<длина>` байт UTF-8.

## `dms-ctl`

| Команда | Что | Путь |
|---|---|---|
| `status` | что отвечает | RCON и канал |
| `exec <команда>` | консольная команда с выводом | RCON (только он отдаёт вывод); `--module` — в `Think` без вывода; если RCON молчит — модуль |
| `lua <код>` / `lua -f <файл>` | Lua в `Think`: что напечатал и вернул (выражение — как `return …`) | модуль; если он не отвечает — RCON `lua_run` |
| `nm <сообщение> [p=v …]` | сообщение NM тестовому агенту | модуль |
| `bench reset\|stop\|cancel` | новый агент / убрать / прервать сценарий | модуль |
| `snapshot [--fields a,b]` | последняя телеметрия (JSON) | модуль |
| `watch --for 2s [--fields a,b] [--every N]` | поток телеметрии, JSON-строка на кадр сервера; **всегда с `--for`** | модуль |
| `run <сценарий> [--out f.rec] [--ref p.rec] [--lead]` | сценарий `golden/scenarios/` на Homer в FOLLOW (`--lead`: LEAD-агент) в реальном времени → `.rec` (по умолчанию `golden/out/source/<имя>.rec`); команды — на шагах записи Playroom (`--ref`, по умолчанию `golden/out/<имя>.rec`) | модуль |

PowerShell 5.1 выкидывает `"` из аргументов внешних программ: в Lua — одинарные кавычки или `lua -f`.

Телеметрия (поля верхнего уровня, `--fields` выбирает): `t` (`CurTime`), `tick`, `frame_dt`, `hover` (сколько
объектов держат отладочные drives `DMS.Drive`), `gravity` (`nm` 9.8, `source` из `physenv`, `compensation` — режим
`dms_gravity_mode`, ADR 0007), `source_gravity`, `nm_ms` (NM на шаг физики, мс), `agents` (FOLLOW-агенты: `id`, `step`,
`awake`, `asleep`, `balancing`, `fallen`, `pelvis_z`, `head_z`, `com`, `com_v`, `compensation`, `parts`, `contacts`,
`drives`, `armature`, `feedback`, `timestep` — шаг физики, `pred_err` — на сколько Box3D разошёлся за шаг с
предсказанием собственной физики NM, `limit_use` — заполнение пределов суставов NM, `state_v` — скорости из
`GetBodyState`, `script`), `bench`; при LEAD-агенте ещё `step`, `nm_t`, `mode`, `held`, `balancing`, `fallen`,
`pelvis_z`, `head_z`, `com`, `com_v`, `parts`, `contacts`, `drives`, `feedback`, `script`.

## Сценарии на Homer в FOLLOW (`module/src/follow.rs`, ROADMAP M4)

`dms-ctl run` (и `golden behaviour --source`) ставит сценарий на **Homer в FOLLOW на Box3D**: Lua спавнит Homer в позе
Playroom в начале комнаты (на 3 м выше для `!button fall`), игрока — туда, где у Playroom камера (4 м сзади), и
отдаёт Homer модулю (`DMS.BenchFollow`); NM — агент на крейте `delirium` с пересылкой outbox и `in_air` Playroom
(ADR 0016, 0017). Команды оконного сценария идут на тех **шагах NM**, где их выполнил Playroom (по меткам `#` его
записи `golden/out/<имя>.rec`, `--ref`; без записи — по времени от первой команды), `!poke` — с импульсом из записи
Playroom. Запись — формат Playroom `--rec`, в порядке его тика; `n`/`g` у частей — держащие контакты NM (`held_world`)
на состоянии Box3D. По окончании NM с Homer снимается (до следующего прогона он обычный ragdoll).

Умеет: сообщения NM, все `!button` поведений Playroom (`Sim::lines`: `balance drunk windmill pedal writhe rollup
catchfall highfall stairs flinch brace look point lean relax stayup foetal grab overwall "STOP all"`, `shot`, `shot
back`; `fall` только в начале; `get-up` переключает режим вставания), выстрел headless на шаге 60 (ShotTask), команды
реакций Playroom (M6, ADR 0020) `!shoot <часть> [fatal]`, `!weapon <i>`, `!blast x,y,z`, `!miss <x>`, `!set <ключ>
<значение>`, `!hfall [up]`, `!nododge`, а также `!poke`, `!quit`. Слой реакций (выстрел, взрыв, смерть, вставание
клипом или мышцами, задачи игры, `in_air`) — тот же код, что у Homer в игре; агент сценария всегда на умолчаниях
Playroom и без тюнинга. Не умеет (пишет `# dms: not run on the bench: …`): `!grab`, Hero, машины Playroom и прочие `!…`.

`golden behaviour --source [имена]` сверяет записи с Playroom **по допускам сценария** — строка `#! tol` в файле
сценария (`golden/README.md`): разрыв COM, конечная высота, падение, время оседания, шаги команд, реакции слоя
комнаты. Сейчас в допусках `stand` (60 с), `stand-1s`, `push`, `relax`, `catchfall`, первые 60 шагов `shot`, стадия
`stairs`; вне допуска `relax-1s`, `lowwall` (близко), конечная поза `kerb`, приземление в `rooftop` (ADR 0019).
`--lead` — прежний LEAD-агент SDK на полу Playroom (как `dms_selftest`): его записи совпадают с Playroom до трёх
знаков, кроме `push` (камера), `catchfall` и `shot`.

`server.ps1 start -Tickrate 60` — тикрейт сервера 60; шаг физики при этом остаётся 0.015 с (его ставит игра),
`DMS.EnvTimestep(physobj[, t])` читает и меняет его (только для опытов).

## Сцена и мир NM (M5, ADR 0018)

`gm_flatgrass`: плоский пол; начало комнаты Playroom — пол Box3D (`DMS_FloorBelow`: плоскость BSP + оболочка форка
6.35 мм) под `info_player_start` (`dms_bench_scene`, `DMS.BenchScene()` проверяет, что пол ровный на ±5 м).

Мир NM — из `.bsp` текущей карты (`world.lua`, `dms_map [reload]`, в логе `[DMS] NM's world: …`); пол — квадратами
1 м по кадру агента, как пол Playroom. `DMS_MapCheck(pos, half, n)` сравнивает высоты мира NM с трассами игры.
Пропы в 3 м от агента — препятствия NM (`obstacles` в телеметрии), `pivot_gap` — разрыв пивотов суставов на
состоянии Box3D.

Стадии Playroom — карта `dms_stages` (её делает `cargo run --release -p dms-map`: `.vmf` в `target/dms-map/`,
`vbsp/vvis/vrad` игры, `garrysmod/maps/dms_stages.bsp`; в git не попадает): `bigstairs`, `stairsdown`, `rooftop`,
`highledge`, `lowwall`, `kerb`, `rail`, `open` и наклон, сайт на стадию (`info_target dms_stage_<имя>`,
`DMS.BenchScene(<имя>)`). Сценарии стадий (`#! window stage=<имя>`) — на стенде с `server.ps1 start -Map dms_stages`;
сценарии комнаты идут на любой из двух карт.
