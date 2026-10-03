# `api-me` — `GET /me`

Zwraca tożsamość wywołującego **tak, jak widzi ją backend**: `sub`, e-mail i grupy z claimów tokenu przekazanych przez autoryzator JWT API Gateway. Frontend pokazuje wynik na stronie głównej, co pozwala sprawdzić cały łańcuch: token z SPA → autoryzator → claimy w Lambdzie.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-api-me` |
| Wyzwalacz | API Gateway HTTP API, trasa `GET /me` (autoryzator JWT) |
| Grupy | każda zalogowana (również bez grupy) |
| Rola IAM | `dam-api-me` — tylko logi (`AWSLambdaBasicExecutionRole`) |
| Pamięć / timeout | 128 MB / 10 s |
| Zmienne | `RUST_LOG` |
| Kod | [`src/main.rs`](src/main.rs) |

## Działanie

1. `shared::http::caller` czyta claimy z kontekstu autoryzatora (`requestContext.authorizer.jwt.claims`).
2. Brak claimów (np. trasa skonfigurowana omyłkowo bez autoryzatora) → **401** i ostrzeżenie w logach. Funkcja nie ufa, że API Gateway zawsze wykona autoryzację.
3. Grupy z claimu `cognito:groups` parsuje `shared::auth::parse_groups` (API Gateway spłaszcza tablicę do napisu `[admin staff]`; nieznane nazwy są pomijane).
4. Odpowiedź `200` z obiektem `Caller`.

```json
{ "sub": "3c4f…", "email": "foto@example.com", "groups": ["admin", "contributor"] }
```

## Bezpieczeństwo

- Funkcja nie ma żadnych uprawnień do danych: nawet przy błędzie nie może niczego odczytać ani zmienić.
- Zwraca wyłącznie dane, które wywołujący i tak ma w swoim tokenie.

## Testy

- `returns_caller_from_claims` — claimy zamieniają się na `sub`/grupy,
- `rejects_request_without_claims` — żądanie bez autoryzatora dostaje 401.
