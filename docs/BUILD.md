# Сборка Dictator из исходников

Этот путь нужен, если на [Releases](https://github.com/LenarBad/dictator/releases/latest) ещё нет zip или вы меняете код. Ниже — **локальная сборка `.app` на Mac**. Windows-установщик собирает GitHub Actions, не этот скрипт ([WINDOWS.md](WINDOWS.md), [INSTALL.md](INSTALL.md)).

Понадобятся Терминал, инструменты Apple, Node и Rust. Сборка занимает **15–40 минут** и около **2 ГБ** места.

Готовый zip / exe без Терминала: [INSTALL.md](INSTALL.md).

## 1. Откройте Терминал

1. Нажмите `Command (⌘) + Пробел` — откроется поиск Spotlight.
2. Наберите `Терминал` или `Terminal`.
3. Нажмите Enter.

Появится окно с текстом. Сюда вы будете **вставлять команды** и нажимать Enter. Команду можно скопировать целиком.

## 2. Инструменты компиляции Apple

```bash
xcode-select --install
```

Появится системное окно. Нажмите **Установить** / **Install** и дождитесь окончания. Если система пишет, что инструменты уже установлены — так и должно быть, идите дальше.

## 3. Homebrew

Homebrew — способ ставить программы одной командой. Проверьте, есть ли он:

```bash
brew --version
```

Если увидели номер версии (например `Homebrew 4.x`) — переходите к шагу 4.

Если `command not found` — установите:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Скрипт спросит пароль пользователя Mac (символы не отображаются — это нормально) и попросит нажать Enter. В конце он **сам напишет** две команды вида `echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile` — выполните их, затем:

```bash
brew --version
```

## 4. Node.js и Rust

```bash
brew install node rust
```

Проверка:

```bash
node --version
rustc --version
```

Должны появиться номера версий. Если нет — закройте Терминал, откройте снова и повторите `brew --version`.

## 5. Скачайте исходники Dictator

**Вариант А — с GitHub, без git.** На странице репозитория нажмите зелёную кнопку **Code → Download ZIP**. Распакуйте архив двойным щелчком. Папка обычно называется `dictator-main` и лежит в «Загрузках».

В Терминале:

```bash
cd ~/Downloads/dictator-main
```

Если папка называется иначе (например `dictator`), подставьте её имя.

**Вариант Б — через git** (если шаг 2 прошёл успешно, git уже есть):

```bash
cd ~
git clone https://github.com/LenarBad/dictator.git
cd dictator
```

## 6. Соберите и установите приложение

Из папки проекта:

```bash
cd desktop
npm install
npm run build:app
```

Первый запуск долгий: качается модель GigaAM и собирается программа. Не закрывайте Терминал. Когда всё готово, в конце будет путь `/Applications/Dictator.app`.

Откройте приложение:

```bash
open /Applications/Dictator.app
```

Дальше — [первый запуск](INSTALL.md#3-первый-запуск-gatekeeper), [разрешения](INSTALL.md#4-разрешения) и [использование](INSTALL.md#5-как-пользоваться), как у готового zip.

Короткий список команд для разработки: [desktop/README.md](../desktop/README.md).

**В Терминале красный текст при сборке.** Чаще всего не хватает шагов 2 или 4. Закройте Терминал, откройте заново, проверьте `node --version` и `rustc --version`, повторите шаг 6.

**`cd: no such file or directory`.** Вы не в той папке. В Finder откройте распакованный архив, перетащите папку в окно Терминала после слова `cd` и пробела, нажмите Enter.
