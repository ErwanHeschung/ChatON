import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { from, Observable } from 'rxjs';
import { ConnectedUser, UserLogin, UserRegister } from '@models/Auth.model';

/** Account commands, all handled by core through Tauri. Errors are `CmdError`. */
@Injectable({
  providedIn: 'root',
})
export class AuthService {
  public register(form: UserRegister): Observable<ConnectedUser> {
    return from(invoke<ConnectedUser>('register', { ...form }));
  }

  public login(form: UserLogin): Observable<ConnectedUser> {
    return from(invoke<ConnectedUser>('login', { ...form }));
  }

  /** The account saved on this device, if any (no login needed). */
  public currentAccount(): Observable<ConnectedUser | null> {
    return from(invoke<ConnectedUser | null>('current_account'));
  }
}
