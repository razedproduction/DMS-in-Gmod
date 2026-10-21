# 0010. Данные NM — прямо из копии игры, кодом Playroom

Статус: принято (2026-10-21). Заменяет в ADR 0006 пункт «пользователь кладёт свои файлы в `garrysmod/data/dms/`».

## Контекст

ADR 0006 предполагал, что пользователь извлекает `fred.xrt` / `character.json`, build и `taskparams.txt` в
`garrysmod/data/dms/`. ADR 0008 требует, чтобы на входе NM были ровно те же данные, что у Playroom. Playroom с
`--game` берёт их из самой игры, и не только файлы:

- на PC выставляет `delirium_data::LE` (little-endian `Fred.art`);
- строит Homer из `componentpeds.img/m_y_pmanhat_01` через `delirium::ragdoll::Rig`;
- выводит из idle-клипа игрока связи частей (`nmdbm::DERIVED_LINKS`, `getup::homer`) — это глобальное состояние,
  которое влияет на поведение агента, но не попадает ни в build-дамп, ни в `character.json`;
- ставит стоячую позу (`Clips::stand_up`), таблицу поверхностей из `materials.dat`, пол CONCRETE;
- в PC-версии `taskparams.txt` нет — работает на умолчаниях.

Через файлы в `data/dms/` (дамп build'а, `character.json`) связи и поверхности теряются: агент был бы другим.

## Решение

1. Данные читаются **из папки игры** (`dms_game_dir`, конвар) при первой нужде, ничего не копируется.
2. Чтение — общий крейт `tools/dms-game`: те же вызовы `delirium-data` / `delirium`, что `delirium-room`
   (`archive_read`, `getup::{key, plat, homer, Clips::stand_up}`, `load`), в том же порядке. Им пользуются модуль и
   `tools/golden`.
3. Модуль передаёт данные в SDK штатно: `nm_init_memory` (персонаж), `nm_build_from_text` (дамп build'а —
   `Build::dump`, формат `NM_DUMP`), `nm_load_task_params`. Связи и поверхности ставятся в те же глобальные
   таблицы, что ставит Playroom.
4. Тождество с Playroom проверяет `tools/golden`: дамп совпадает с `NM_DUMP` байт в байт, связи — бит в бит по
   прогону Playroom с `NM_LINKS`, агент в LEAD — бит в бит с первой секундой Playroom.
5. Поддерживаются PC и Xbox 360 (ключ xex берётся там, где его хранит Playroom); PS3 — нет.

## Последствия

- Утилита «сделать `character.json` из файлов игры» не нужна; `character.json` не используется.
- Данные грузятся один раз за процесс (так устроен Delirium): смена `dms_game_dir` требует перезапуска.
- M3 (`dms-homer`) строит модель из того же `dms-game`.
