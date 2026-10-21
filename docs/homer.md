# Модель Homer (ROADMAP M3)

`models/dms/homer.mdl` — ragdoll, физика которого — тело NM Homer один в один (ADR 0013). Модель делается **у вас**
из вашей копии GTA IV и никуда не распространяется (ADR 0006).

## Сделать

```powershell
cargo build --release -p dms-homer
.\target\release\dms-homer.exe --game "D:\Games\Grand Theft Auto IV"
```

`--game` можно не указывать, если задан `DMS_GAME_DIR` или есть `golden\game_dir.txt`; Garry's Mod ищется по Steam
(или `--gmod <папка GarrysMod>`). Что делает:

1. читает Homer (`m_y_pmanhat_01`) кодом Playroom (`tools/dms-game`): build, стоячая поза, тело NM;
2. пишет `target\dms-homer\homer_ref.smd` (вид как в Playroom: капсулы/боксы NM, левые части красные, правые синие,
   остальные серые, полупрозрачные), `homer_phys.smd` (оболочки капсул/боксов NM), `homer.qc` (схемы цвета Playroom —
   скины: 0 mixed, 1 all blue, 2 all red, 3 solid blue, 4 mixed opaque), материалы
   `garrysmod\materials\models\dms\homer\*.vmt`; меш и текстуры GTA IV не используются;
3. компилирует `studiomdl` из `bin\win64` GMod → `garrysmod\models\dms\homer.*`;
4. переписывает `.phy`: массы NM, центр масс в центре части, диагональ инерции NM, демпфирование 0, констрейнты,
   пары self-collision NM; печатает сверку `.phy` с телом NM.

## В игре

- `dms_homer` (суперадмин) — Homer стоит в позе Playroom перед игроком, лицом к нему; суставы сразу получают пивоты,
  кадры и лимиты NM и тензоры инерции NM (`DMS.BindHomer`). NM им пока не управляет (FOLLOW — M4): он падает как обычный ragdoll.
- Lua: `DMS_SpawnHomer(pos, yaw)`, `DMS_HomerParts(ent)`, `DMS_HomerReport(ent)`; натив `DMS.HomerBody([pos, yaw])`.
- Заспавненная из меню модель не связана с NM (пивоты в центрах частей) — спавнить через `dms_homer`.

## Проверка

```powershell
.\scripts\server.ps1 start
.\target\release\golden.exe body          # --no-game: без игры
.\scripts\server.ps1 stop
```

`golden body`: build против `NM_DUMP` Playroom и C API SDK (1e-5); `.phy` против build'а; в игре — кадры 21 тела
(≤ 1 мм, ≤ 0.5°), массы, инерция (≤ 1 %), FindConstraint на 20 суставах, затем падение без NM 4 с (всё конечно,
пивоты вместе, суставы в пределах NM ± 5°).

Нужен форк с `IPhysicsNM` `VBOX3D_NM_003` (ADR 0014): `DMS.BindHomer` ставит каждой части полный тензор инерции NM
(`SetBodyInertia`), шарам — семантику NM (`SetJointBallLimits`), хинжам — `SetJointLimits`.

## Визуальная проверка (пользователю)

1. `dms_homer` перед собой: Homer стоит в позе Playroom, лицом к вам, выглядит как в Playroom — полупрозрачные
   капсулы, левые красные, правые синие, середина серая (не фиолетово-чёрные клетки, не вывернутые грани); скины
   1–4 (`ent:SetSkin(n)`) — остальные схемы Playroom.
2. `vcollide_wireframe 1`: 21 капсула/бокс сидят внутри тела (ступни и кисти — боксы), ничего не торчит.
3. Падение: Homer оседает и лежит, не дрожит, не разлетается, колени и локти гнутся только в свою сторону.
4. Лица и скелета пока нет (M6).
5. Физганом потаскать за руку/голову — суставы держатся.
