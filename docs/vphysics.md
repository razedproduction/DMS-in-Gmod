# Форк VPhysics-Box3D в `vphysics/`

`vphysics/` — форк [Asphaltian/VPhysics-Box3D](https://github.com/Asphaltian/VPhysics-Box3D) (замена `vphysics.dll`
GMod на Box3D) с NM-расширением `IPhysicsNM` (`include/vphysics_nm.h`, сейчас `VBOX3D_NM_004`). Он живёт в этом
репозитории как **git subtree** с полной историей (ADR 0015): правки модуля и физики идут одним PR, upstream берётся
`git subtree pull`. Прежний отдельный репозиторий `razedproduction/VPhysics-Box3D` больше не обновляется.

Лицензия форка — MIT, файл `vphysics/LICENSE` (остальной репозиторий — GPL-3.0-only).

## Что где

- `vphysics/include/vphysics_nm.h` — интерфейс; копия для FFI — `module/src/ffi/vphysics_nm.h` (сверять при каждом
  изменении интерфейса).
- `vphysics/src/physics_nm.*` — реализация `IPhysicsNM`; остальные NM-правки — в `physics_object.*`,
  `physics_constraint.*`, `physics_environment.cpp` (step-callback) и `cmake/patches/*.patch` (патчи Box3D:
  armature суставов, эллиптический конус, дополнительные проходы по суставам с armature и контактам тел NM).
- Сабмодули `vphysics/external/box3d` (erincatto/box3d) и `vphysics/external/source-sdk-2013` (ValveSoftware) —
  записаны в корневом `.gitmodules` (shallow). `vphysics/.gitmodules` — файл upstream'а, git его не читает; оставлен,
  чтобы `subtree pull` не конфликтовал.
- `vphysics/.gitattributes` (`*.patch text eol=lf`) действует внутри `vphysics/` как есть: без `eol=lf` патчи на
  Windows получат CRLF и `git apply` не наложит их на Box3D.

## Сборка и установка

```powershell
git submodule update --init --depth 1   # один раз после clone
.\scripts\build-vphysics.ps1            # -> vphysics\build\windows-x64\src\Release\vphysics.dll
.\scripts\install.ps1 -Fork             # модуль + аддон + форк -> GarrysMod\bin\win64\vphysics.dll
```

`build-vphysics.ps1` — это `cmake --preset windows-x64` + `cmake --build --preset windows-x64-release` внутри
`vphysics/` (нужны CMake ≥ 3.25 и Visual Studio 2026). После `subtree pull` или переключения на другой коммит
собирать с `-Clean`: инкрементальная сборка поверх другого коммита уже давала битый dll (heap corruption при
загрузке карты). `install.ps1 -Fork` при первой установке сохраняет Steam'овский dll как `vphysics.dll.original` и
больше его не трогает. `build/` форка в git не попадает (`vphysics/.gitignore`).

Только `windows-x64`. AVX2 Box3D включает только во время выполнения (`wide8.c` с `/arch:AVX2`, выбор по cpuid),
на процессоре без AVX2 работает обычный путь — флаги сборки не трогать.

## Правило правок

Форк меняется только ради того, чтобы NM работал один в один как в Playroom (ADR 0008): сначала показать
расхождение (что делает Delirium, что делает форк), потом чинить. Новые/изменённые методы — новой версией
интерфейса (`VBOX3D_NM_004`…) с сохранением старых строк, заголовок копировать в модуль. Не связанное с NM не менять
— иначе каждый `subtree pull` будет конфликтовать.

## Правки под физику NM в M4 (ADR 0017)

Интерфейс не менялся (`VBOX3D_NM_003`), изменилось поведение для тел и суставов под `IPhysicsNM`:

- `box3d-joint-armature.patch`: `Cdot0` armature берётся один раз на сабстеп (до интегрирования сил), а не на каждом
  проходе релаксации — иначе второй проход отменял импульс armature и при > 1 проходе тело разносило.
- `box3d-armature-iterations.patch`: `b3World_SetArmatureIterations` — дополнительные проходы Гаусса–Зейделя только по
  суставам с armature (её ставит лишь `IPhysicsNM`) перед каждой итерацией решения и релаксации; форк ставит 8
  (`NM_ARMATURE_ITERATIONS`). Почти жёсткое дерево Homer (armature до 9 кг·м²) иначе не сходится за один проход.
- `CPhysicsObject::GetBodyState` внутри шага отдаёт скорость начала шага (`CaptureNMStepStart`): «core velocity» к
  step-callback уже содержит гравитацию шага от контроллера IVP (покоящееся тело читалось как падающее 0.23 м/с).
- Тела под NM всегда берут непрерывную гравитацию Box3D (`FlushVelocity`), а не толчок `g·dt` контроллера в начале шага.

Проверка после каждой правки физики — `golden body` и `golden behaviour --source` на стенде.

## Правки под мир NM в M5 (ADR 0018, `VBOX3D_NM_004`)

- Новые методы в конце vtable (строки `VBOX3D_NM_003…001` по-прежнему отдают тот же объект):
  `SetBodyMaterial(obj, μ, e)` — материал части NM (фигуры тела получают его трение и упругость, в `userMaterialId` —
  тег `NM_BODY_MATERIAL` и прежний surfaceprop); `SetSurfaceMaterial(surfaceprop, μ, e)` — значения NM для
  surfaceprop (таблица в `physics_nm.cpp`, на все окружения).
- `MixFriction`/`MixRestitution` (`physics_environment.cpp`): часть NM × поверхность с материалом NM (или часть ×
  часть) — `μA·μB` без обрезки и `min(eA·eB, 1)`, как физика NM; всё остальное — прежнее смешивание surfaceprop.
- `CPhysicsConstraint::ApplyStiffnessBudget`: сустав между двумя телами NM не проходит бюджет жёсткости, коррекция
  дрейфа — потолок Box3D (¼ частоты сабстепов), ζ = 0.2 (`NM_JOINT_*`). Голеностоп под весом тела тянется ~6 мм
  вместо ~9.
- `box3d-armature-contacts.patch` (ADR 0019): `b3Body_SetArmatureContacts` — контакты тела не красятся, а идут в
  overflow (скалярный путь) и решаются ещё раз в каждом доп. проходе после суставов (`b3SolveArmatureContacts`; без
  прибавки к `totalNormalImpulse`). Форк ставит флаг в `CPhysicsObject::SetNMDriven`. Прочие контакты игры — в цветах,
  как в upstream. Стационарное растяжение суставов NM (~5 мм) — упругость мягкого сустава `F / (m_eff · ω²)`, не
  сходимость (ADR 0019).

## Свежий upstream Asphaltian

```powershell
git fetch origin
git switch -c vphysics-upstream origin/main
git subtree pull --prefix vphysics https://github.com/Asphaltian/VPhysics-Box3D main -m "vphysics: merge upstream Asphaltian/VPhysics-Box3D <коммит>"
git submodule update --init --depth 1   # если upstream сдвинул Box3D/SDK
.\scripts\build-vphysics.ps1 -Clean
```

Без `--squash`: история upstream'а входит целиком, как при `subtree add`. Если upstream сдвинул сабмодуль, gitlink
`vphysics/external/box3d` обновится вместе со слиянием; корневой `.gitmodules` менять не нужно (только если upstream
поменял URL или добавил сабмодуль — тогда перенести это в корневой с префиксом `vphysics/`).

Проверка после слияния: патчи Box3D наложились (configure пишет `Patching Box3D`, без `Failed to apply`), игра
отдаёт `VBOX3D_NM_004` (`DMS.InterfaceVersion()`), `golden body` на стенде проходит и цифры пивотов/лимитов не
хуже прежних (сравнить с прогоном до слияния). PR — как обычно, описание с таблицей конфликтов.

### Конфликты NM-правок

NM-правки сидят в тех же местах, что и активная работа upstream'а, поэтому конфликты ожидаемы. Как их разрешили при
слиянии `9d5e14c` (бывший razedproduction/VPhysics-Box3D#2, коммит `98e1130` в истории `vphysics/`):

| Файл | Конфликт | Решение |
|---|---|---|
| `cmake/Box3D.cmake` | NM собирает Box3D из пропатченной копии в build-дереве; upstream убрал `set(BOX3D_* OFF)` | `add_subdirectory("${BOX3D_DIR}" "${CMAKE_BINARY_DIR}/box3d-build" EXCLUDE_FROM_ALL)` (отдельный binary dir — исходники внутри build), `set(... OFF)` убраны как в upstream |
| `physics_constraint.h` | NM: `m_flArmature`; upstream: новые/переименованные поля | поля upstream + `m_flArmature` |
| `physics_constraint.cpp` (`CreateJoint`) | NM: `b3Joint_SetArmature` после создания; upstream: пороги разрыва, `QueueStiffnessUpdate` | сначала armature, затем блок upstream без изменений |
| `physics_object.h` | NM: `GetBodyState`/`ApplyDrive`/`SetInertiaTensor`; upstream: stiffness budget | обе группы методов |
| `physics_environment.cpp` (`Step`) | NM: step-callback перед `b3World_Step`; upstream: `UpdateStiffnessBudgets()`, `CaptureBulletSweeps()` | сначала NM-callback, потом код upstream (бюджеты видят суставы, пересозданные NM, sweeps — позы до шага) |

Общий принцип: код upstream'а берётся как есть, NM-вставки ставятся рядом; порядок в шаге — NM-callback первым.

Слияние может менять поведение и без конфликтов. После `9d5e14c` пивоты Homer в `golden body` разошлись с 0.8 до
2.1–2.5 мм; бисекция нашла upstream-гейт `IsContactActive` (`5ddb06c`: контакт без трения гасится, если тела
расходятся). Исправлено флагом `m_bNMDriven` у `CPhysicsObject` (ставят методы `IPhysicsNM`, управляющие телом), такие
тела гейт пропускают (коммит `4543837`); интерфейс не менялся. Поэтому `golden body` — обязательная часть проверки
каждого слияния.
