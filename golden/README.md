# golden — эталоны Playroom

Playroom (`delirium-room` из SDK) — golden для DMS (ADR 0008). `tools/golden` собирает его, снимает эталоны и
проверяет, что DMS берёт ровно те же входные данные NM. Всё, что лежит в `golden/out/`, сделано из файлов игры
пользователя — **не коммитится** (ADR 0006, `.gitignore`); в репозитории только сценарии и инструмент.

## Одна команда

```powershell
cargo run --release -p golden -- --game "E:\Downloads\TorrentDownloads\Grand Theft Auto IV" all
```

`--game` запоминается в `golden/game_dir.txt` (тоже не в git), дальше можно без него; или переменная
`DMS_GAME_DIR`. Каталог SDK по умолчанию — `..\delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)`, иначе `--sdk`.

| Команда | Что делает | Выход в `golden/out/` |
|---|---|---|
| `build` | `cargo build --release -p delirium-room --locked --manifest-path <SDK>\Cargo.toml --target-dir target-playroom`, запуск из этого репозитория; до и после — `git status` и `Cargo.lock` SDK, любое изменение — ошибка | `target-playroom\release\delirium-room.exe` |
| `dump` | `NM_DUMP` Playroom (`--game … --headless 0`: Homer = `m_y_pmanhat_01` после `getup::homer` + `stand_up`) против build'а, который строит DMS (`tools/dms-game`); должно совпасть **байт в байт** | `homer.nmdump`, `homer.ours.nmdump` |
| `links` | связи, которые DMS выводит из idle-клипа (`DERIVED_LINKS`), подаются Playroom через `NM_LINKS`; его 60 шагов должны совпасть бит в бит с прогоном без них | `homer.links`, `links.*.frames` |
| `lead` | агент SDK в LEAD, собранный как в `dms_selftest` (build из дампа, пол комнаты Playroom CONCRETE — плитки 1 м с ямой бассейна, `world::floor_tris`, seed 0.3, `bodyBalance`), против первой секунды Playroom — бит в бит | `lead.dms.frames` |
| `run [имена]` | сценарии `golden/scenarios/*.txt` (все или названные) | `<имя>.rec`, `<имя>.log`, `<имя>.frames` |
| `all` | всё по порядку | |
| `body [--no-game]` | тело Homer, как его портирует `dms-homer` (M3, [docs/homer.md](../docs/homer.md)): build против `NM_DUMP` и C API SDK (1e-5), `homer.phy` против build'а, в игре на стенде — кадры, массы, инерция, падение без NM | — |

Playroom запускается с рабочим каталогом `golden/out/` (его служебные файлы ложатся туда, в SDK ничего не пишется) и
с `ROOM_LOG=1` (журнал в `<имя>.log`). Сборка Playroom идёт без `.cargo/config.toml` SDK (там `target-cpu=x86-64-v3`
с AVX2): у пользователя процессор без AVX2.

## Как Playroom исполняет сценарии (важно для M4)

У Playroom **два** режима, и `--headless` **не исполняет `--script`** (сценарий разбирает только оконная система
`scripted`):

1. **headless** — `--headless <N> [--press a,b,…] [--rec файл]`: без окна, детерминированно. На шаге 0 он сам
   жмёт кнопку `balance` (`bodyBalance start=true`), затем кнопки из `--press` по порядку; **на шаге 60 (1.00 с)
   всегда стреляет** в грудь (`shot`: часть 9 Spine2, оружие по умолчанию — pistol из `weaponinfo.xml`, ShotTask
   Playroom). В stdout — кадр на шаг: 21 часть × 7 float (`x y z qx qy qz qw`) битами hex (`{:08x}`), части через
   ` | ` (`<имя>.frames`). Это точный эталон (бит в бит).
2. **window** — `--script <файл> [--rec файл]`: окно Playroom, команды по **часам** (`time.elapsed_secs()`), шаги
   NM идут в реальном времени (1/60, аккумулятор, не больше 0.25 с за кадр). Пока не пришла первая команда, тело
   держится в позе и шаги не идут (`held`), поэтому сценарии начинаются на 3 с — после загрузки окна. Между
   командами всё детерминировано, а шаг, на котором сработала команда, может гулять на ±1 кадр — **фактический шаг
   каждой команды записан в `.rec`** строкой `# <t> …` (t = шаг/60). Сверять нужно по этим отметкам, а не по
   времени из файла сценария. `<t> !quit` закрывает окно; `golden` останавливает Playroom, если он завис.
   `golden` запускает окно с **`--safe`** (Safe Mode, с v0.2.1): без него Playroom применяет пользовательский
   `%APPDATA%\delirium\settings.txt`, где есть и переключатели поведения (`look`, `avoid`, `extra_reactions`, …), а
   на обычной графике окно идёт ~6 кадров/с и теряет шаги (60 с стойки — лишь 1436 шагов вместо 3600). Safe Mode —
   умолчания Playroom и минимальная графика; на физику не влияет. Первая строка журнала тогда — `# 0.00 room  Safe
   Mode: …` (в сверке отметок команд не участвует).

Выключенная анимация вставания: Playroom после 2 с лежания поднимает тело клипом `get_up_fast` (это не NM).
Кнопка `get-up` перебирает режимы `animation → muscles → off → learned`; два нажатия = `off`, поэтому сценарии
начинаются с `get-up` ×2 (`press=get-up,get-up` в headless).

## Формат сценария `golden/scenarios/<имя>.txt`

Это файл `--script` Playroom плюс строка режима для `golden`:

```
# комментарий (всё после # игнорируется)
#! headless <шагов> [press=кнопка,кнопка]     ← либо
#! window                                      ← и тогда строки команд:
<t, с> <команда>
```

Разбор строки — ровно как в Playroom (`main`, `--script`): всё после первого `#` отбрасывается, строка обрезается
по краям, пустые пропускаются; затем `split_once(' ')` — время и команда разделены **одним пробелом** (не табом),
время — `f32`; строка с неразбираемым временем молча пропускается. Строки сортируются по времени устойчиво (одно
время — в порядке файла). Команда срабатывает в первом кадре, где `время ≤ прошедшего времени`; все созревшие
команды кадра выполняются по порядку **до** шагов NM этого кадра. Внутри команды `#` использовать нельзя. Строки
`#!` для Playroom — комментарии, их читает только `golden`. Команды (`scripted` в `delirium-room/src/main.rs`):

- **сообщение NM** — строка без `!`: `<сообщение> [параметр=значение …]`, как в консоли Playroom: `bodyBalance
  start=true`, `catchFall start=true`, `applyImpulse partIndex=9 hitPoint=x,y,z impulse=x,y,z`. `start` — bool;
  остальные параметры по типу (`nm_message_param_kind`): float/int числом, bool `true/false/1/0`, vec3 `x,y,z`.
  Отправляется агенту через `msg::send` (как `nm_msg_send`) и снимает `held`.
- `!button <кнопка>` — кнопка панели Playroom: поведения `balance drunk windmill pedal writhe rollup catchfall
  highfall stairs shot flinch brace look point lean relax stayup foetal grab overwall "STOP all"` (что именно они
  шлют — `Sim::lines`: `balance` = `bodyBalance start=true`, `relax` = `bodyRelax start=true`, `catchfall` =
  `catchFall start=true` …), а также `shot` / `shot back`, `fall` (новый агент на 3 м выше + `smartFall`),
  `push front` / `push back` (плита 1.2 м/с), `RESET pose`, `get-up`, `crowd`, `car`, `ice` и др.
- `!poke <часть>` — `applyImpulse` 8 Н·с в часть от камеры (`eye` = (0, −4, 1.5)).
- `!shoot <часть> [fatal]` — выстрел (`Sim::fire`) в часть.
- `!grab <часть> dx,dy,dz` / `!release` — тянуть часть к точке / отпустить.
- `!blast x,y,z` — взрыв; `!strength <k>` — `agent.strength`; `!walk x,y`; `!pause`; `!quit` — выход.
- остальные `!…` (`!cam`, `!crane`, `!photo`, `!screenshot`, `!hero…`, `!taxi_run`, `!portal …`) — камера,
  съёмка, Hero, машины, порталы; для эталонов не нужны.

Сценарии сейчас:

| Сценарий | Режим | Что |
|---|---|---|
| `stand-1s` | headless 60 | стойка `bodyBalance`, до выстрела на шаге 60 |
| `relax-1s` | headless 60 | `balance`, затем `relax` на шаге 0 |
| `shot` | headless 600 | стойка, выстрел в грудь на 1.00 с, 10 с |
| `stand` | window | стойка `bodyBalance` 60 с |
| `push` | window | стойка, через 2 с `!poke 9` (8 Н·с в грудь) |
| `catchfall` | window | `fall` (3 м, `smartFall`) и сразу `catchFall start=true` |
| `relax` | window | `bodyRelax` из стойки |

## Формат `--rec`

Первая строка — заголовок `# game: frame t pelvis xyz |v| touches | ours: t com xyz |v|   (# lines: the log)`.
Дальше на каждый шаг NM:

```
- - - - - - - | <t> <com x> <com y> <com z> <|v com|> | p<k> <x> <y> <z> n<касаний> g<зазор> [<точки>] | p<k> …
```

- строка пишется после шага NM (`tick`), `t` = номер шага после него / 60, т.е. первая строка — `0.017`;
- поля «game» (`- - - - - - -`) в Playroom пустые; COM тела (x y z) и модуль его скорости (`doll.com()`);
- две самые низкие части по z центра кадра (`frames()`, устойчивая сортировка по z): `p<k>` — индекс части NM,
  центр x y z, `n<число>` — удерживающих контактов (`held_world(k)`), `g<зазор>` — минимальный зазор среди них
  (9.000, если контактов нет), в `[...]` — точки контактов `x+1.5,y,z` через пробел (сдвиг 1.5 по x — так пишет
  Playroom); после второй — ` | crowd x y z` на каждого Homer толпы (в сценариях толпы нет);
- все числа — `{:.3}` (три знака после точки, `-0.000` возможен), разделители — ровно как в шаблоне: ` | ` между
  группами, пробел внутри;
- строки `# <t> <текст>` — журнал Playroom (`Sim::say`), `t` — `{:.2}` текущего номера шага / 60 (до шага, в
  котором событие повлияет): нажатые кнопки (`room  <кнопка>`), отправленные сообщения (`ours  MSG <строка>`),
  feedback NM (`ours  FB <имя> slot <n>`, каждый шаг заново), события (`room  shot …`, `room  poke …`). Первой
  идёт `# 0.00 weaponinfo.xml: N weapons` (в headless её нет).

Headless-прогоны дополнительно дают `.frames` (stdout): на шаг одна строка, 21 часть в порядке NM, у каждой 7
значений `x y z qx qy qz qw` как `{:08x}` битов `f32`, внутри части — пробел, между частями — ` | `. Это кадр после
`tick` и `rise` шага — то, что сравнивается бит в бит.

`dms_script` в GMod (M4) должен выполнять те же команды в те же **шаги** (по отметкам `#` из `.rec` эталона) и писать
запись в этом же формате; `tools/golden behaviour` сравнит их.
