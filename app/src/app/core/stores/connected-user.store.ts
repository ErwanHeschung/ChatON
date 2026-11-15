import { signalStore, withState, withMethods, patchState } from '@ngrx/signals';
import { inject } from '@angular/core';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, CmdError, ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { catchError, of, pipe, switchMap, tap } from 'rxjs';

export const ConnectedUserStore = signalStore(
  { providedIn: 'root' },
  withState({
    user: null as ConnectedUser | null,
    status: AuthStatus.LOADING,
    /** Android push (UnifiedPush). `unsupported` on desktop, which notifies while running. */
    push: 'unsupported' as 'on' | 'pending' | CmdError['code'] | 'unsupported',
  }),
  withMethods((store, authService = inject(AuthService)) => {
    /** Asks core for this device's push endpoint and registers it on my instance. */
    const enablePush = async () => {
      patchState(store, { push: 'pending' });
      try {
        await invoke('enable_push');
        patchState(store, { push: 'on' });
      } catch (e) {
        patchState(store, { push: (e as CmdError).code ?? 'internal' });
      }
    };
    return {
      enablePush,
      refresh: rxMethod<void>(
        pipe(
          switchMap(() => {
            patchState(store, { status: AuthStatus.LOADING });
            return authService.currentAccount().pipe(
              tap((user) => {
                patchState(store, {
                  user,
                  status: user ? AuthStatus.AUTHENTICATED : AuthStatus.UNAUTHENTICATED,
                });
                if (user) void enablePush();
              }),
              catchError(() => {
                patchState(store, { user: null, status: AuthStatus.UNAUTHENTICATED });
                return of(null);
              }),
            );
          }),
        ),
      ),
      setLoggedInUser(user: ConnectedUser) {
        patchState(store, {
          user,
          status: AuthStatus.AUTHENTICATED,
        });
        void enablePush();
      },
    };
  }),
);
