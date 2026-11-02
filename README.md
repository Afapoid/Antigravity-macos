# Antigravity Unlocker — macOS Apple Silicon

Форк [confeden/Antigravity](https://github.com/confeden/Antigravity) с нативной терминальной сборкой для Mac на M1, M2 и более новых Apple Silicon. Rosetta и Homebrew для запуска не нужны.

## Запуск на Mac

```sh
curl -fsSL https://raw.githubusercontent.com/Afapoid/Antigravity-macos/main/tui.sh | sh
```

Команда скачивает ARM64-релиз, проверяет SHA-256 и подпись, затем открывает TUI. Повторный запуск обновляет программу при появлении нового релиза. При недоступности GitHub запускается сохранённая версия. Запускайте от обычного пользователя, без `sudo`.

Установите Antigravity в `/Applications` или `~/Applications`. Анлокер также ищет CLI в `~/.gemini/bin`, `~/.local/bin`, `~/.agy` и каталогах Homebrew. Другую установку можно указать вручную в TUI.

Ключ активации по-прежнему требуется: используется механизм и версия ключей оригинального проекта. Инструкция автора: [README.upstream.md](README.upstream.md). Стрелки выбирают пункт, пробел переключает, `q` выходит.

## Что адаптировано

- Нативный `aarch64-apple-darwin`, macOS 11 и новее.
- Пути `*.app/Contents/Resources`, в том числе на файловых системах с учётом регистра.
- Поиск Language Server с суффиксом платформы и загруженного IDE бэкенда `agy`.
- Изменённые Mach-O-файлы подписываются локальной ad-hoc подписью перед атомарной заменой. После патча обновляется подпись `.app`.
- Прокси и автопатч работают как пользовательские LaunchAgents, без systemd и root.
- `AG_LS_PROXY` задаётся через launchd для приложений, открываемых из Finder. После включения полностью закройте и заново откройте Antigravity. Для запуска CLI из уже открытого терминала: `AG_LS_PROXY=http://127.0.0.1:53129 agy` (порт может измениться при конфликте).
- Настройки и логи находятся в `~/Library/Application Support/AGUnlocker`, агенты — в `~/Library/LaunchAgents/org.agunlocker.*.plist`.
- Если файлы приложения недоступны для записи, macOS запросит пароль только для патча этой установки.

Закрытые маршруты и адреса автора не входят в публичный исходный код. Форк не добавляет их и не обещает работоспособность регионального обхода в любой сети. Можно настроить свой прокси через TUI. Windows NRPT и изменение системного DNS на macOS не реализованы.

Переключатели TUI снимают патчи и удаляют пользовательские агенты. После изменения бинарников исходная подпись разработчика заменяется локальной; для возврата именно исходной подписи переустановите Antigravity.

## Сборка из исходников на Mac

Нужны Rust и Xcode Command Line Tools (`xcode-select --install`).

```sh
git clone https://github.com/Afapoid/Antigravity-macos.git
cd Antigravity-macos
sh macos/build.sh
```

Архив и SHA-256 появятся в `release/`. Сборка TUI выполняется с `--no-default-features`; исходный GUI остаётся в коде для совместимости и не входит в релиз Mac.

```sh
python3 -m unittest discover -s tests -v
cargo test --locked --no-default-features -- --test-threads=1
```

GitHub Actions собирает и проверяет программу на ARM64-раннере macOS. Тег `v<версия>` публикует архив и контрольную сумму в Releases.

Исходная [лицензия](LICENSE) и авторство сохранены. Форк основан на upstream-коммите `05cadba`.
