import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { from, Observable } from 'rxjs';
import { ConnectedUser, LinkJoin, UserLogin, UserRegister } from '@models/Auth.model';

/** Account commands, all handled by core through Tauri. Errors are `CmdError`. */
@Injectable({
  providedIn: 'root',
})
export class AuthService {
  public register(form: UserRegister): Observable<ConnectedUser> {
    return from(invoke<ConnectedUser>('register', { ...form }));
  }

  /** Signs in with a saved secret key (nsec). */
  public importKey(form: UserLogin): Observable<ConnectedUser> {
    return from(invoke<ConnectedUser>('import_key', { ...form }));
  }

  /** New device: gets the key from a signed-in device showing `code`. */
  public linkJoin(form: LinkJoin): Observable<ConnectedUser> {
    return from(invoke<ConnectedUser>('link_join', { ...form }));
  }

  /** Signed-in device: a code for the new device. Then `link_request` and `link_done` events. */
  public linkOffer(): Observable<string> {
    return from(invoke<string>('link_offer'));
  }

  public linkConfirm(accept: boolean): Observable<void> {
    return from(invoke<void>('link_confirm', { accept }));
  }

  public linkCancel(): Observable<void> {
    return from(invoke<void>('link_cancel'));
  }

  public exportKey(): Observable<string> {
    return from(invoke<string>('export_key'));
  }

  /** Forgets the account on this device. */
  public logout(): Observable<void> {
    return from(invoke<void>('logout'));
  }

  /** The account saved on this device, if any (no login needed). */
  public currentAccount(): Observable<ConnectedUser | null> {
    return from(invoke<ConnectedUser | null>('current_account'));
  }
}
