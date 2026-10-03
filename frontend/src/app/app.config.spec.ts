import { formatDate } from '@angular/common';
import { LOCALE_ID, ValueProvider } from '@angular/core';

import { createAppConfig } from './app.config';
import { TEST_RUNTIME_CONFIG } from './testing/fake-oidc';

describe('createAppConfig', () => {
  it('uses the Polish locale for dates', () => {
    const providers = createAppConfig(TEST_RUNTIME_CONFIG).providers as ValueProvider[];
    const locale = providers.find((p) => p?.provide === LOCALE_ID)?.useValue;

    expect(locale).toBe('pl');
    const date = new Date(2026, 9, 3, 14, 48);
    expect(formatDate(date, 'short', locale)).toBe('3.10.2026, 14:48');
  });
});
