import { bootstrapApplication } from '@angular/platform-browser';

import { App } from './app/app';
import { createAppConfig } from './app/app.config';
import { loadRuntimeConfig } from './app/core/config/runtime-config';

loadRuntimeConfig()
  .then((runtime) => bootstrapApplication(App, createAppConfig(runtime)))
  .catch((error: unknown) => {
    console.error(error);
    const message = document.createElement('p');
    message.className = 'p-6 text-error';
    message.textContent =
      'Nie udało się uruchomić aplikacji: brak lub błędny /config.json. Lokalnie uruchom `just frontend-config`.';
    document.body.replaceChildren(message);
  });
