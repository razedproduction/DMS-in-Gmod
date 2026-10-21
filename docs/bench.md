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
  `gmod_win64.exe -textmode -nosound -novid -usercon -condebug -ip 127.0.0.1 -port 27115 +maxplayers 2 +sv_lan 1
  +sv_cheats 1 +sv_hibernate_think 1 +rcon_password <новый> +map gm_flatgrass`; окно скрывается (`ShowWindow`).
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
| `run <сценарий> [--out f.rec]` | сценарий `golden/scenarios/` в реальном времени → `.rec` (по умолчанию `golden/out/source/<имя>.rec`) | модуль |

PowerShell 5.1 выкидывает `"` из аргументов внешних программ: в Lua — одинарные кавычки или `lua -f`.

Телеметрия (поля верхнего уровня, `--fields` выбирает): `t` (`CurTime`), `tick`, `frame_dt`, `hover` (сколько
объектов держат drives `step.rs`), `gravity` (`nm` 9.8, `source` из `physenv`, `compensation` — `null` до M4,
ADR 0007), `agents` (FOLLOW-агенты, M4: пока `[]`), `bench`; при тестовом агенте ещё `step`, `nm_t`, `mode`, `held`,
`balancing`, `fallen`, `pelvis_z`, `head_z`, `com`, `com_v`, `parts` (21 × `x y z qx qy qz qw`), `contacts`, `drives`,
`feedback`, `script`.

## Тестовый агент и сценарии (`module/src/bench.rs`)

До FOLLOW (M4) на стенде нет Homer'а на Box3D, поэтому сценарии исполняет **агент SDK в LEAD на полу Playroom**
(как `dms_selftest` и `golden lead`): NM двигает своё тело, физика игры не участвует. Логика — как окно Playroom:
команды по времени от начала сценария, до шагов кадра; шаги NM 1/60 по аккумулятору (≤ 0.25 с за кадр), не
раньше первой команды (`held`). Headless-сценарии: `balance` и `press=` на шаге 0, шагов столько, сколько в `#!`.
Запись — формат Playroom `--rec` (`golden/README.md`); `n` у частей — флаг `nm_agent_get_part_contact`, зазор 9.000
и без точек (держащие контакты Playroom `held_world` C API не отдаёт; в FOLLOW касания будут из Box3D).

Умеет: сообщения NM, `!button` (`balance windmill pedal writhe rollup catchfall highfall stairs relax stayup
foetal "STOP all"`, `fall` из позы build'а, `get-up` — только журнал, `RESET pose`), `!poke`, `!quit`. Не умеет
(пишет `# dms: not run on the bench: …`): выстрел (`shot`, выстрел headless на шаге 60 — ShotTask Playroom, M6),
кнопки, которым нужен взгляд тела (`drunk flinch brace look point lean grab overwall`), `!shoot`, `!grab`, `!blast`,
`!strength` и прочие `!…`.

`golden behaviour --source [имена]` прогоняет сценарии на стенде и сравнивает с эталоном `golden/out/<имя>.rec`:
COM, |v|, нижние части по шагам (допуск — три знака записи), шаги команд (строки `#`). Сейчас совпадают `stand`,
`stand-1s`, `relax`, `relax-1s`. Не совпадают: `shot` (выстрела нет), `push` (`!poke` Playroom бьёт от своей
камеры, которая в окне движется; стенд — от её начальной точки `(0, −4, 1.5)`), `catchfall` (Playroom шлёт агенту
сообщения из `extras.outbox` — `pedalLegs`, `bodyRollUp`; C API SDK их не отдаёт — вопрос M4). У оконных сценариев
шаг команды может гулять на ±1 кадр, как и в самом Playroom.

## Сцена

`gm_flatgrass`: плоский пол; начало комнаты Playroom — пол под `info_player_start` (`dms_bench_scene`,
`DMS.BenchScene()` проверяет, что пол ровный на ±5 м). Стадии Playroom (лестница, стена, уступ, мост) — M5 вместе с
миром NM из карты (в игре есть `vbsp/vvis/vrad`, карту можно будет собрать).
