# `api-dictionaries-write` — edycja słowników

Tworzenie, zmiana i usuwanie wpisów słowników przez administratora:

- `PUT /dictionaries/{kind}/{id}`: utworzenie lub zastąpienie wpisu,
- `DELETE /dictionaries/{kind}/{id}`: usunięcie wpisu.

`kind` to `players`, `seasons`, `competitions`, `matches` albo `sponsors`. `id` to slug nadany przez A (np. `jan-kowalski`, `2025-26`). Frontend proponuje go z nazwy, ale ostatecznie decyduje A.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-dictionaries-write` |
| Wyzwalacz | API Gateway, obie trasy (rozróżnienie po metodzie) |
| Grupy | A |
| Rola IAM | `dam-dictionaries-write` |
| Pamięć / timeout | 128 MB / 10 s |
| Zmienne | `DICTIONARIES_TABLE` |
| Kod | [`src/main.rs`](src/main.rs), walidacja: [`shared/src/dictionary.rs`](../../shared/src/dictionary.rs) |

## Pola wpisów

| Rodzaj | Pola ciała (bez `id`, które jest w ścieżce) |
|---|---|
| `players` | `name` (wymagane), `number` (1–99, opcjonalnie), `position` (`GOALKEEPER`, `DEFENDER`, `MIDFIELDER`, `FORWARD`, opcjonalnie), `active` (domyślnie `true`) |
| `seasons` | `name` (np. `2025/26`) |
| `competitions` | `name` |
| `matches` | `seasonId`, `competitionId` (slugi istniejących wpisów), `opponent`, `date` (`RRRR-MM-DD`), `home` (`true`/`false`) |
| `sponsors` | `name` |

## `PUT` krok po kroku

1. Autoryzacja: tylko A.
2. `kind` musi być znanym rodzajem (inaczej 404), `id` slugiem: małe litery ASCII, cyfry i myślniki, maks. 64 znaki, bez myślnika na początku (inaczej 400).
3. `DictionaryEntry::parse`:
   - `deny_unknown_fields`: nieznane pole → 400; pole `id` w ciele → 400 (id podaje się w ścieżce);
   - nazwy przycięte, niepuste, maks. 100 znaków, bez znaków sterujących i `<>`;
   - numer 1–99, data z poprawnym dniem miesiąca (z latami przestępnymi).
4. **Referencje meczu**: `GetItem` (odczyt silnie spójny) dla sezonu i rozgrywek. Brak któregoś → 400 `Nie ma wpisu seasons/…`.
5. `PutItem` rekordu `{ kind: "MATCH", id, data: "<JSON>" }`, który zastępuje istniejący wpis.
6. Log `dictionary entry saved` z rodzajem, id i `sub` admina.

## `DELETE` krok po kroku

1. Autoryzacja, `kind` i `id` jak wyżej.
2. **Sezon lub rozgrywki używane przez mecz** → 409 (`Query` partycji `MATCH`). Najpierw trzeba zmienić albo usunąć mecze.
3. `DeleteItem` z warunkiem `attribute_exists(id)`. Brak wpisu → 404.
4. Log `dictionary entry deleted` na poziomie `WARN`.

Assety mogą odwoływać się do usuniętego zawodnika czy meczu. Celowo tego nie blokujemy, bo przeszukiwanie wszystkich assetów przy każdym usunięciu byłoby drogie. Frontend pokazuje wtedy identyfikator zamiast nazwy, a A może poprawić metadane.

## Uprawnienia (`dam-dictionaries-write-main`)

| Akcja | Zasób | Po co |
|---|---|---|
| `dynamodb:GetItem` | `dictionaries` | istnienie sezonu i rozgrywek meczu |
| `dynamodb:PutItem` | `dictionaries` | zapis wpisu |
| `dynamodb:DeleteItem` | `dictionaries` | usunięcie |
| `dynamodb:Query` | `dictionaries` | mecze używające sezonu lub rozgrywek |

## Odpowiedzi

| Kod | Kiedy |
|---|---|
| 200 | `{ "kind": "players", "id": "jan-kowalski" }` |
| 400 | niepoprawny slug, nieznane pole, pusta lub za długa nazwa, `<>` w nazwie, zły numer lub data, mecz bez istniejącego sezonu lub rozgrywek |
| 403 | nie A |
| 404 | nieznany rodzaj słownika; `DELETE` nieistniejącego wpisu |
| 409 | usunięcie sezonu lub rozgrywek, do których odwołuje się mecz |

## Testy

- `only_admins_can_edit_dictionaries`;
- `invalid_requests_never_reach_dynamodb`: nieznany rodzaj, zły slug, `<script>`, numer 0, data 30 lutego;
- walidację wpisów testuje `shared::dictionary`, a e2e dodatkowo kolejność usuwania (sezon używany przez mecz → 409).
