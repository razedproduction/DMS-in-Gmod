# DMS in Gmod

Delirium (active ragdoll, NM-style) нативно в Garry's Mod в режиме **FOLLOW**: тело — обычный `prop_ragdoll` на
физике [VPhysics-Box3D](../VPhysics-Box3D) (форк с `IPhysicsNM` и armature суставов), Delirium каждый физический
шаг возвращает силы и моменты для частей.

> Проект разрабатывается с помощью [Claude Code](https://claude.com/claude-code).

- [ROADMAP.md](ROADMAP.md) — план разработки по вехам M0–M9.
- [docs/adr/](docs/adr/README.md) — архитектурные решения.

## Сборка

Только x86-64 ветка GMod (Windows):

```powershell
.\scripts\build.ps1
```

Результат — `dist\gmsv_dms_win64.dll`. Установка в игру (dll → `garrysmod\lua\bin`, `addon` →
`garrysmod\addons\dms`; GMod ищется по пути Steam из реестра, или `-GarrysMod <путь>`):

```powershell
.\scripts\install.ps1
```

## Лицензия

GPL-3.0-only, см. [LICENSE](LICENSE).
