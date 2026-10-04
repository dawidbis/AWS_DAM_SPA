# `api-dictionaries-read` — `GET /dictionaries`

Zwraca wszystkie słowniki klubu naraz: zawodników, sezony, rozgrywki, mecze i sponsorów. Frontend trzyma je w pamięci i na ich podstawie wyświetla nazwy na kafelkach assetów (zamiast identyfikatorów) oraz listy wyboru w edytorze metadanych.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-dictionaries-read` |
| Wyzwalacz | API Gateway, `GET /dictionaries` |
| Grupy | A, B, C, D (użytkownik bez grupy → 403) |
| Rola IAM | `dam-dictionaries-read` |
| Pamięć / timeout | 128 MB / 10 s |
| Zmienne | `DICTIONARIES_TABLE` |
| Kod | [`src/main.rs`](src/main.rs), model: [`shared/src/dictionary.rs`](../../shared/src/dictionary.rs) |

## Działanie

1. Autoryzacja: dowolna grupa A–D.
2. `Scan` całej tabeli `dictionaries` ze stronicowaniem. Słowniki mają dziesiątki wpisów, więc jeden skan jest tańszy i prostszy niż pięć zapytań.
3. Każdy rekord to `kind` + `id` + `data` (JSON wpisu). `DictionaryEntry::from_item` odrzuca uszkodzone rekordy (ostrzeżenie w logu), więc jeden zły wpis nie psuje odpowiedzi.
4. `Dictionaries::from_entries` grupuje i sortuje:
   - zawodnicy po numerze (bez numeru na końcu),
   - sezony i mecze od najnowszych,
   - rozgrywki i sponsorzy alfabetycznie.
5. **Sponsorów dostaje tylko A.** Sponsor z grupy D nie powinien widzieć, z kim jeszcze klub współpracuje. Lista sponsorów przyda się w części 3 etapu 3 (widoczność assetów dla wybranych sponsorów).

## Odpowiedź

```json
{
  "players": [{ "id": "michal-kruk", "name": "Michał Kruk", "number": 9, "position": "FORWARD", "active": true }],
  "seasons": [{ "id": "2025-26", "name": "2025/26" }],
  "competitions": [{ "id": "liga", "name": "Liga Regionalna" }],
  "matches": [{ "id": "2025-09-13-unia-lesna", "seasonId": "2025-26", "competitionId": "liga", "opponent": "Unia Leśna", "date": "2025-09-13", "home": true }],
  "sponsors": []
}
```

## Uprawnienia (`dam-dictionaries-read-main`)

| Akcja | Zasób |
|---|---|
| `dynamodb:Scan` | `dictionaries` |

## Testy

`only_admins_see_sponsors`, `users_without_a_group_are_forbidden`. Sortowanie i parsowanie rekordów testuje `shared::dictionary`.
