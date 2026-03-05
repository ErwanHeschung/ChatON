import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { toObservable } from '@angular/core/rxjs-interop';
import { provideRouter } from '@angular/router';
import { AuthStatus } from '@models/Auth.model';
import { filter, firstValueFrom } from 'rxjs';
import { ConnectedUserStore } from 'src/app/core/stores/connected-user.store';
import { routes } from './app.routes';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    provideAppInitializer(() => {
      const store = inject(ConnectedUserStore);
      store.refresh();
      const status$ = toObservable(store.status);
      return firstValueFrom(status$.pipe(filter((status) => status !== AuthStatus.LOADING)));
    }),
  ],
};
