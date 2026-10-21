# 0001. Модуль на Rust, Delirium SDK линкуется как `rlib` по пути

Статус: принято (2026-10-21)

## Контекст

Delirium написан на Rust. Его SDK (`crates/delirium-sdk`, крейт `delirium_physics`) собирается и как `cdylib`
(C API `delirium_physics.h`), и как `rlib`. Каталог `delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)` менять
нельзя. GMod загружает бинарные модули только из `garrysmod/lua/bin` и ищет зависимые DLL по стандартным путям
Windows, где `lua/bin` не участвует.

## Решение

- Бинарный модуль `gmsv_dms` пишется на Rust (`module/`), крейт `cdylib`.
- SDK подключается path-зависимостью
  `delirium-sdk = { path = "../../delirium-physics (CLAUDE DONT CHANGE ANYTHING HERE)/crates/delirium-sdk" }`
  и вызывается как обычные Rust-функции (`delirium_physics::nm_*`).
- Сборка идёт в `target/` этого репозитория, каталог SDK только читается.
- API Lua (LuaJIT из `lua_shared`) берётся в рантайме через `GetProcAddress`, без сторонних крейтов-обёрток.

## Последствия

- Одна DLL на архитектуру, без отдельного `delirium_physics.dll` рядом с игрой.
- Модуль обязан собираться той же версией Rust, что поддерживает SDK (edition 2024).
- DLL экспортирует и символы `nm_*` из SDK — это безвредно, но их не надо считать частью API модуля.
- Обновление SDK — это `git pull` в его каталоге и пересборка, без копирования кода.
