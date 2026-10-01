# 0013. GitHub OIDC, role plan/deploy i permission boundary

- Status: zaakceptowany
- Data: 2026-10-01

## Kontekst

CI musi wdrażać infrastrukturę, w tym tworzyć role IAM dla Lambd. Nie chcemy kluczy dostępowych w GitHub, a rola zdolna tworzyć role IAM jest klasyczną drogą eskalacji uprawnień.

## Decyzja

**Uwierzytelnienie:** dostawca OIDC `token.actions.githubusercontent.com`, warunki `aud = sts.amazonaws.com` i dokładny `sub`.

| Rola | `sub` w trust policy | Uprawnienia |
|---|---|---|
| `dam-github-plan` | `repo:<repo>:pull_request` | `ReadOnlyAccess` + zapis/usuwanie tylko plików `*.tflock` w buckecie stanu |
| `dam-github-deploy` | `repo:<repo>:ref:refs/heads/main` | `PowerUserAccess` + `dam-github-deploy-iam` (role i polityki `dam-*`) |

**Permission boundary `dam-permissions-boundary`** jest przypięte do obu ról GitHub i do każdej roli tworzonej przez deploy. Efektywne uprawnienia to część wspólna polityk roli i boundary. Boundary:

1. dopuszcza tylko usługi używane w projekcie i tylko region projektu (+ `us-east-1` dla usług globalnych),
2. pozwala tworzyć role wyłącznie z tym samym boundary (`iam:PermissionsBoundary`), więc nie da się utworzyć „czystej" roli i jej przyjąć,
3. zabrania zdejmowania boundary, zmiany samej polityki boundary oraz jakichkolwiek zmian ról i polityk `dam-github-*`,
4. chroni bucket stanu przed usunięciem i zmianą polityki, wersjonowania, szyfrowania.

IAM write jest ograniczony do ARN-ów `role/dam-*` i `policy/dam-*`; `iam:PassRole` tylko do usług projektu (`iam:PassedToService`).

## Dlaczego `PowerUserAccess` zamiast własnej listy akcji

W każdym etapie dochodzą nowe usługi i akcje. Ręczna lista akcji dla deployu szybko się rozjeżdża i kończy wklejaniem `*`. Tu szeroka polityka tożsamościowa jest świadomie zawężona przez boundary, które jest małe, czytelne i stabilne. To „uzasadniony wyjątek" z rozdziału 8. Role Lambd nadal mają polityki least privilege, boundary jest dla nich tylko drugą linią obrony.

## Rozważane alternatywy

- **Access keys w GitHub Secrets** — zakazane (rozdział 8).
- **`AdministratorAccess` dla deployu** — prostsze, ale deploy mógłby zmienić własną rolę i zdjąć ograniczenia.
- **Własna, wąska polityka deployu** — docelowo możliwa (IAM Access Analyzer na podstawie CloudTrail), do rozważenia w etapie 4.

## Konsekwencje

- Każda rola w `infra/envs/*` musi mieć `permissions_boundary` (moduł `rust-lambda` robi to automatycznie), inaczej `CreateRole` kończy się `AccessDenied`.
- Zmiany ról GitHub i boundary wymagają administratora i `infra/bootstrap`.
- Ochrona gałęzi `main` jest częścią modelu bezpieczeństwa: kto może wypchnąć do `main`, ten może wdrażać.
