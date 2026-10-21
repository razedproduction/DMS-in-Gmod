# Данные NM: копия GTA IV

Delirium нужны данные персонажа из GTA IV. В аддоне их нет и быть не может (ADR 0006): DMS читает их **прямо из
вашей копии игры** теми же функциями `delirium-data` и в том же порядке, что Playroom с `--game` (ADR 0010).
Ничего не копируется и не сохраняется.

Подходит PC-версия (папка с `GTAIV.exe` и `pc\`) или Xbox 360 (папка с `default.xex`; ключ xex Playroom кладёт в
`%APPDATA%\delirium\xex.key` при первом запуске с этой игрой — DMS его только читает). PS3 не поддерживается.

## В GMod

В консоли сервера (или суперадмином):

```
dms_game_dir "D:\Games\Grand Theft Auto IV"
dms_data
dms_selftest
```

`dms_game_dir` сохраняется (`FCVAR_ARCHIVE`). Данные грузятся один раз за запуск — Delirium держит персонажа в
глобальном состоянии; чтобы сменить папку, перезапустите GMod.

Что читается:

| Что | Откуда (PC) | Откуда (Xbox 360) | Зачем |
|---|---|---|---|
| персонаж NM | `pc\data\naturalmotion\Fred.art` | `xbox360.rpf` → `data/naturalmotion/fred.xrt` | суставы, массы, лимиты, self-collision (`nm_init_memory`) |
| тело Homer | `pc\models\cdimages\componentpeds.img` → `m_y_pmanhat_01.wft` | то же, `.xft` | build через `delirium::ragdoll::Rig` |
| поза стойки и связи | `pc\models\cdimages\playerped.rpf` → `player.wft`, `pc\anim\anim.img` → `move_player.wad` / `idle` | `xbox360.rpf`, `anim.img` | `derive_links`, `stand_up` |
| поверхности | `common\data\materials\materials.dat` | `common.rpf` | трение/упругость (`SURFACES`), пол — строка CONCRETE |
| тюнинг задач | `common\data\taskparams.txt` | `common.rpf` | в PC-версии его нет — NM на умолчаниях, как и Playroom |

На PC файлы игры little-endian: перед чтением ставится `delirium_data::LE`, как это делает Playroom.

## Без данных

`dms_selftest`, `dms_data`, `dms_nm_build`, `dms_nm_joints` без `dms_game_dir` (или с неверной папкой) пишут, чего
не хватает и что указать. `DMS.LoadData(dir)` в Lua в этом случае бросает ошибку с тем же текстом.

## Что совпадает с Playroom

`tools/golden` (`golden/README.md`) проверяет: build Homer, который строит DMS, совпадает с `NM_DUMP` Playroom байт
в байт; выведенные связи дают Playroom тот же прогон бит в бит; агент DMS в LEAD (как в `dms_selftest`) идёт бит в
бит с первой секундой Playroom.
