# Lua API DMS (M6)

Homer в режиме FOLLOW: NM (Delirium) ведёт ragdoll `models/dms/homer.mdl`, а слой реакций Playroom (ADR 0016,
0020) отвечает на то, что с ним делает игра. Всё — на сервере; менять что-либо из консоли и меню может только
суперадмин.

## Быстрый старт

```lua
-- Homer из меню спавна привязывается сам (dms_autoattach 1); свой — dms_homer или DMS_SpawnHomer(pos, yaw).
local c = dms.Attach(rag)          -- NM на ragdoll + bodyBalance (кнопка Playroom "balance")
c:Shot(9, nil, "shotgun")          -- выстрел дробовиком в грудь: applyImpulse + ShotTask Playroom
c:Send("headLook", { start = true, pos = ply:EyePos() })
print(c:GetState().health)         -- 200 у живого Homer, урон — ped::damage игры
dms.Detach(rag)                    -- обычный ragdoll
```

## `dms.*`

| Функция | Что делает |
|---|---|
| `dms.Attach(rag[, opts])` | NM на Homer (`opts.balance = false` — без `bodyBalance`). Контроллер или `nil, причина`. Ragdoll должен быть привязан к телу NM (спавн через `dms_homer`, `DMS_SpawnHomer` или меню спавна) |
| `dms.Detach(rag)` | NM снят: armature 0, drives нет, обычный ragdoll |
| `dms.Get(rag)` | контроллер Homer с NM или `nil` |
| `dms.All()` | контроллеры всех Homer с NM |
| `dms.Messages()` | имена всех сообщений (поведений) NM |
| `dms.MessageParams(name)` | параметры сообщения: `{name, kind, used, doc, default}` (`used = false` — порт принимает, но не использует) |
| `dms.Reactions()` | `{buttons, extra, getup, weapons}` — кнопки Playroom, дополнительные реакции, режимы вставания, оружие |

## Контроллер

| Метод | Что делает |
|---|---|
| `c:Send(msg[, params])` | сообщение NM, как его берёт консоль Playroom: `c:Send("bodyBalance start=true")` или `c:Send("bodyBalance", {start = true})`. `Vector` — точки игры (переводятся в кадр NM), для направлений (`dir`, `normal`, `bulletVel`, `impulse`, `vel`, `fallOverEdge`) — как есть; строка `"x,y,z"` — кадр NM без перевода |
| `c:React(line)` | реакция слоя комнаты Playroom (строка `DMS.FollowReact`, ниже) |
| `c:Shot(where[, dir][, weapon])` | выстрел: `where` — часть 0–20 или точка (`Vector`), `weapon` — `pistol deagle smg ak47 sniper shotgun rpg grenade molotov bat knife taser baton` |
| `c:Button(name)` | кнопка Playroom (`balance`, `catchfall`, `flinch`, `brace`, `relax`, `STOP all`, …, `shot`, `shot back`) |
| `c:Balance()`, `c:CatchFall()`, `c:Relax()`, `c:Stop()` | кнопки `balance`, `catchfall`, `relax`, `STOP all` |
| `c:Flinch([from])` | `upperBodyFlinch` от точки или кнопка `flinch` |
| `c:Kill()`, `c:SetHealth(h)` | здоровье 0 (смерть: `stopAllBehaviours` + `bodyRelax`) / любое |
| `c:GetState()` | `{steps, awake, asleep, balancing, fallen, pelvis, head, com, com_v, health, dead, rising, burning, shock, getup}` (м, таблицы `{x, y, z}`) |
| `c:GetParam(msg, param)` | описание параметра сообщения |
| `c:Origin()` | начало кадра NM (м, кадр игры) |

## Хуки

| Хук | Аргументы | Когда |
|---|---|---|
| `DMS_Attached` | `ent, id` | NM надет |
| `DMS_Detached` | `ent, why` | NM снят (`detached`, или NaN/разнос — причина) |
| `DMS_Fallen` | `ent` | балансир NM сдался |
| `DMS_Feedback` | `ent, name, slot, value` | feedback NM (после каждого шага NM) |
| `DMS_Died` | `ent` | здоровье 0: Homer обмяк |
| `DMS_Reaction` | `ent, text` | строка журнала слоя комнаты Playroom (`room  shot (pistol) at part 9 …`) |

## Реакции (`DMS.FollowReact(id, line)`, `dms_react`)

Точки — `x,y,z` в метрах, кадр игры. Что Playroom делает на каждую — `module/src/follow/room.rs` (порт `Sim`).

| Строка | Реакция Playroom | Что шлёт NM |
|---|---|---|
| `shot [part=k] [at= dir=] [weapon=] [fatal=1] [more=k,k]` | `fire` | урон оружия (`ped::damage`, `weaponinfo.xml`), `applyImpulse` пули (`shottask::bullet`), ShotTask с набором taskparams оружия (`weapon_set`); `more` — остальные дробины |
| `blast at=` | `blast_on` | `applyImpulse` каждой части в 5 м (15 м/с · масса · (1 − d/5)), урон 250 · (1 − d/5), задача взрыва 6 с |
| `burn` | `burn` | 25 урона в секунду; задача NM OnFire (`fire_reaction`) |
| `taser [part=k] [at=]` | `zap` | `electrocute start=true stunMag=0.4 largeMult=2` на 2.5 с |
| `melee damage= at= impulse= head=` | `land` (удар Hero) | `applyImpulse`, задача ближнего боя игры: flinch (`MELEE`/`MELEE_PASIVE`) или balance |
| `nearmiss from= dir=` | `near_miss` | в 1.5 м от головы/груди: `bodyBalance`, `upperBodyFlinch`, `headLook`; лёжа — `bodyFoetal` (доп. реакции); `dive_shot` — нырок |
| `car impulse= at= [id=] [player=1]` | `car_react` | урон `bump::damage`, задача `bump::react`: brace / rollup / balance / relax |
| `prop at= impulse=` | `obj_hit` | урон 0.6 за Н·с (1–35), ушиб конечности (`injury`) |
| `thrown from= vel=` | `throw` | `headLook` (`look`), `upperBodyFlinch` (`avoid`) |
| `dropped` | `fall` | `smartFall` (доп. реакции) |
| `button name=<кнопка>` | `button` | сообщения кнопки (`Sim::lines`); `STOP_all` — `STOP all` |
| `health value=`, `kill` | — | здоровье; `kill` — смертельный выстрел |

Игра переводится в реакции сама (`addon/lua/dms/reactions.lua`): `EntityTakeDamage` (пули → `shot`, дробь одного
тика → один `shot` с `more`, взрыв → `blast`, огонь → `burn`, удар → `melee`, шокер → `taser`), `PhysicsCollide`
(машина → `car`, проп → `prop`), `EntityFireBullets` (промах → `nearmiss`), физган и гравипушка (`thrown`,
`dropped`). Толчок пули и взрыва — `applyImpulse` Playroom, поэтому собственная сила урона GMod по Homer обнуляется.

## Переключатели Playroom (конвары, умолчания Playroom)

| Конвар | По умолчанию | Что |
|---|---|---|
| `dms_tasks` | 0 | задачи игры: NM → анимация, когда задача кончается |
| `dms_look`, `dms_avoid` | 1, 0 | смотрит на брошенное / уклоняется |
| `dms_getup` | 0 | вставание: 0 клип игры, 1 мышцы (NMPose по клипу), 2 не встаёт, 3 обученная сеть (не перенесена) |
| `dms_extra_reactions`, `dms_extra_pick` | 0, 255 | дополнительные реакции v0.2.1 и их выбор (биты: windmill, curl up, curl up под огнём, взгляд на ударившего, кувырок с уступа, smartFall при броске, уклонение сверху, упор в стену) |
| `dms_fire_reaction` | 1 | OnFire при горении |
| `dms_dive_shot`, `dms_dodge_gap` | 0, 3 | нырок при повторном обстреле |
| `dms_weak`, `dms_god` | 0, 0 | слабые наборы оружия, бесконечное здоровье |

Сценарии стенда и golden всегда на умолчаниях Playroom (их меняет только `!set` сценария).

## Тюнинг (ADR 0009)

`dms_tuning 0|1`, `dms_tuning_profile <имя>`; профили — `garrysmod/data/dms/profiles/*.json`
(`{name, messages = {[сообщение] = {[параметр] = "значение"}}, taskparams = {["задача набор параметр"] = число}}`),
только отличия от Playroom; «Playroom» — пустой и неизменяемый. Значения профиля добавляются к сообщениям, которые
запускают поведение или меняют его параметры, если строка сама их не задаёт; taskparams — поверх файла игры.
`dms_tuning_status` — активные значения. Меню: «Утилиты → DMS → Homer (Delirium)».

## Консоль

| Команда | Что |
|---|---|
| `dms_homer` | Homer перед игроком, NM, `bodyBalance` |
| `dms_attach` | NM на Homer, на которого смотрит игрок |
| `dms_detach_all` | NM со всех Homer |
| `dms_send <строка>` | сообщение NM Homer под прицелом (с консоли сервера — всем) |
| `dms_react <реакция> [key=value …]` | реакция Homer под прицелом; без `at=` — точка прицела |
| `dms_status`, `dms_tuning_status` | агенты, тюнинг |
| `dms_react_log 1` | печатать журнал слоя комнаты |

## Клиент

`dms_eyes`, `dms_brows` (лицо Playroom: глаза моргают, смотрят на место удара или по ходу, щурятся от боли,
закрыты у мёртвого; брови — испуг, тревога, боль), `dms_skeleton` (скелет Playroom), `dms_overlay` (суперадмин:
капсулы и боксы NM, силы и моменты drives, COM и его проекция; зелёный — балансирует, красный — упал),
`dms_pedcol 0–4` (сервер: схемы цвета Playroom — скины модели).
