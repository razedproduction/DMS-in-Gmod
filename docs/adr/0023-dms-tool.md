# 0023. Инструмент DMS для toolgun

Статус: принято (2026-10-27). Уточняет 0016, 0022.

## Контекст

В окне DMS (ADR 0022) реакции и поведения пробуются «на Homer, на которого смотришь», без точки попадания; привязать
NM к конкретному Homer, посмотреть его состояние, направить его взгляд или руку на точку можно только из консоли.
Пользователь попросил всё это одним инструментом toolgun.

## Решение

`addon/lua/weapons/gmod_tool/stools/dms_tool.lua` — один инструмент (категория «DMS» во вкладке Tools), режим
выбирается в его панели (`dms_tool_mode`); панель показывает только опции режима и перестраивается при смене режима
откуда угодно (список, консоль, бинд). Подсказки ЛКМ/ПКМ/R меняются вместе с режимом.

| Режим | ЛКМ | ПКМ | R |
|---|---|---|---|
| NM on / off | `dms.Attach` | `dms.Detach` | кнопка Playroom «balance» |
| Reactions | реакция Playroom **в точке попадания** (оружие, урон и импульс удара — из панели) | `kill` | `health value=200` |
| NM behaviour | `<поведение> start=true` (с профилем тюнинга, если он включён) | кнопка «STOP all» | «balance» |
| Target | точка в мире для выбранного Homer (держишь — он следит) | выбрать Homer (без выбора — ближайший) | `<сообщение> start=false` |
| Inspector | выбрать Homer: состояние и последние события NM на экране, отладочный оверлей только на нём | снять выбор | оверлей вкл/выкл |
| Kick | держишь — набирается сила, отпустил — удар Playroom (`melee`) в часть под прицелом | — | — |
| Throw Homer | новый Homer перед игроком, брошен по прицелу, затем реакция `dropped` | новый Homer стоя в точке прицела | — |
| Health | `health value=<ползунок>` | `kill` | `health value=200` |
| Pins | верёвка: часть Homer, затем якорь | снять его крепления | шарнир части к миру в точке |

- Строки — как у Playroom (`delirium-room`): `headLook start=true pos= alwaysLook=true`, `pointArm start=true target=`,
  `leanInDirection start=true leanAmount=0.4 dir=`, `upperBodyFlinch start=true pos= newHit=true`,
  `grab start=true linegrab=true useLeft=true useRight=true dontLetGo=true pos1= pos2=` (линия 0.6 м поперёк взгляда).
  Точки переводятся в кадр NM через `Controller:Send`.
- Реакции в точке — общая функция `DMS_ReactAt` (`reactions.lua`), её же теперь использует `dms_react`; спавн Homer
  для игрока (undo, cleanup, NM, `bodyBalance`) — `DMS_SpawnHomerFor` (`homer.lua`), её же — `dms_homer`.
- Инспектор шлёт текст состояния владельцу 5 раз в секунду, только пока инструмент в руках и режим «Inspector»;
  оверлей ограничивается выбранным Homer через `ply.DMS_OverlayOnly` (`overlay.lua`).
- Список поведений и оружия в панели — из каталога окна (`DMS_MenuCatalogue`, хук `DMS_Catalogue`, `cl_menu.lua`).
- Доступ: только суперадмины, пока серверный конвар `dms_tool_everyone` не равен 1; спавн Homer — всегда только
  суперадмины (как `dms_homer`).
- Крепления — констрейнты игры (`constraint.Rope`, `constraint.Ballsocket`). Мир NM о них не знает: закреплённый
  Homer ведёт себя уже не как в Playroom — это песочница поверх, в панели режима об этом сказано.

## Последствия

- Всё, что делает инструмент, идёт через входы Playroom и публичный API; NM не тюнится, golden-прогоны не затронуты.
- Новый режим — строка в `MODES`, ветка в `TOOL:Act` и опции в `BuildCPanel`.
- Не сделано: бессмертие для одного Homer (`dms_god` — общий для всех; в комнате модуля нет здоровья на агента вне
  её таблицы), «хватание» за движущийся проп (`grab` идёт в точку мира, без `instanceIndex`).
