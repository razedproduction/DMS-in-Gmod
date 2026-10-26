# 0015. Форк VPhysics-Box3D — в репозитории DMS через git subtree

Статус: принято (2026-10-26).

## Контекст

Форк VPhysics-Box3D с `IPhysicsNM` жил отдельным репозиторием `razedproduction/VPhysics-Box3D` (форк
Asphaltian/VPhysics-Box3D) с локальной копией `../VPhysics-Box3D`. Любая правка физики ради совпадения с Playroom
(ADR 0008, 0014) шла двумя PR в два репозитория: интерфейс в форке, его копия и использование — в модуле. Какой
коммит форка нужен какому коммиту модуля, нигде не было записано; golden-сверка проверяла сборку, которую нельзя было
восстановить по истории DMS.

## Решение

Один репозиторий: форк — папка `vphysics/` в `razedproduction/DMS-in-Gmod`, добавлен
`git subtree add --prefix vphysics https://github.com/razedproduction/VPhysics-Box3D.git main` (`43789dd`) **без
`--squash`** — история форка и upstream'а сохраняется.

- Сабмодули форка (`external/box3d`, `external/source-sdk-2013`, shallow) записаны в корневой `.gitmodules` с
  префиксом `vphysics/`, на тех же коммитах. `vphysics/.gitmodules` и `vphysics/.gitattributes` остаются как у
  upstream'а (второй действует в своей папке: `*.patch` с `eol=lf`).
- Свежий upstream: `git subtree pull --prefix vphysics https://github.com/Asphaltian/VPhysics-Box3D main`
  ([docs/vphysics.md](../vphysics.md)).
- Сборка — `scripts\build-vphysics.ps1` (CMake-пресеты форка внутри `vphysics/`), установка —
  `scripts\install.ps1 -Fork`.
- Правило ADR 0008 не меняется: форк правится только ради совпадения с Playroom, новые методы — новой версией
  интерфейса.

Отвергнуто: сабмодуль на форк (те же два PR и ручное обновление ссылки), `--squash` (теряется история и бисекция по
коммитам upstream'а, которая уже понадобилась при слиянии `9d5e14c`).

## Последствия

- Правка физики и модуля — один PR; коммит DMS однозначно задаёт и модуль, и `vphysics.dll`.
- `razedproduction/VPhysics-Box3D` больше не обновляется (его можно заархивировать), локальная `../VPhysics-Box3D`
  не нужна.
- История репозитория включает всю историю VPhysics-Box3D. Её коммиты до `subtree add` лежат с путями без префикса
  `vphysics/`, поэтому `git log -- vphysics` их не показывает: они видны через вторых родителей слияний subtree
  (`git log cd0f52b^2`, `git log -- src/physics_nm.cpp`).
- Форк — MIT (`vphysics/LICENSE`), остальное — GPL-3.0-only (ADR 0006); `vphysics.dll` распространяется по MIT.
- NM-правки, не нужные upstream'у, остаются локальными: каждый `subtree pull` может конфликтовать в местах NM-вставок
  (разбор — в docs/vphysics.md).
