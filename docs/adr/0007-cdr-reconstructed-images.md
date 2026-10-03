# 0007. CDR i publikacja zrekonstruowanej wersji zamiast oryginału

- Status: zaakceptowany
- Data: 2026-10-03

## Kontekst

Antywirus wykrywa tylko znane złośliwe oprogramowanie. Plik bez sygnatury może nadal nieść zagrożenie: XSS w polu EXIF/XMP, treść doklejoną za obrazem (poliglota, który jest jednocześnie GIF-em i HTML-em), exploity parserów obrazów, bomby dekompresyjne. Rozdział 7.2 wymaga, żeby do galerii trafiała wyłącznie zrekonstruowana wersja pliku.

## Decyzja

- **Walidacja przed CDR** (Lambda `validate`): typ pliku z magic bytes (`infer`), tylko JPEG, PNG i WebP, zgodność z typem zadeklarowanym w `upload-init`, rozmiar do 200 MB, wymiary odczytane z nagłówka (`imagesize`) bez dekodowania pikseli: najwyżej 20 000 px na bok i 100 MP. Bomba dekompresyjna jest odrzucana, zanim cokolwiek ją rozpakuje.
- **CDR** (Lambda `cdr`): obraz dekodowany do pikseli (crate `image`, limity wymiarów i 1 GB pamięci dekodera) i kodowany od nowa w tym samym formacie: JPEG z jakością 90, PNG, WebP bezstratnie. W nowym pliku nie ma EXIF/XMP/IPTC, profilu ICC, miniatur ani doklejonej treści. Orientacja z EXIF jest stosowana do pikseli.
- **Whitelista metadanych**: autor (`Artist`), prawa autorskie (`Copyright`) i data wykonania (`DateTimeOriginal`) są czytane z oryginału, sanityzowane (bez znaków sterujących i `<>`, do 200 znaków) i zapisywane jako atrybuty assetu. Nie osadzamy ich z powrotem w pliku, więc plik publikowany nie zawiera żadnych metadanych od użytkownika.
- **Typ, wymiary, rozmiar i SHA-256** w tabeli `assets` pochodzą z pipeline'u, nie od klienta (rozdział 5).
- **Kolejność kroków: najpierw antywirus, potem walidacja i CDR** (odwrotnie niż w szkicu z rozdziału 3.2). Znane złośliwe oprogramowanie w pliku o złym typie (np. plik wykonywalny jako `.jpg`) ma skończyć się incydentem i alertem (`INFECTED`), a nie samym odrzuceniem (`REJECTED`), które nie zostawia śladu w `incidents`.
- **Bez PDF**: dla PDF nie ma w Ruście sensownego CDR (rekonstrukcja wymagałaby rasteryzacji albo przepisania struktury dokumentu). Zamiast publikować oryginał, PDF wypada z listy dozwolonych typów. Wróci, gdy powstanie CDR dla dokumentów.
- **Limit uploadu 200 MB** (zamiast 1 GB z rozdziału 3.4): CDR dekoduje obraz w pamięci Lambdy, a większe pliki i tak zostałyby odrzucone przez walidację.

## Konsekwencje

- Odrzucenie przez walidację lub CDR kończy się statusem `REJECTED` z powodem w `rejectReason`. Błąd lub timeout kroku kończy się `SCAN_FAILED` (ADR 0008).
- Ponowne kodowanie JPEG jest stratne (jakość 90) i usuwa profil ICC, więc kolory szerokiej gamy mogą się nieznacznie zmienić. To świadomy koszt bezpieczeństwa.
- Lambda `cdr` ma 2 GB pamięci; obraz 100 MP to rząd kilku sekund i kilkunastu GB-s, w ramach Free Tier.
