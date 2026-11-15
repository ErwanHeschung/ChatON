import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';
import { Router } from '@angular/router';
import { CmdError, ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { LucideCircleAlert, LucideDynamicIcon } from '@lucide/angular';
import { finalize } from 'rxjs';
import { ROUTES } from '@app/app.routes';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';

interface LoginForm {
  handle: FormControl<string>;
  nsec: FormControl<string>;
}

/**
 * Step 4: imports an existing key for a handle (checked against the directory).
 * Email + password login comes with the vault (step 5).
 */
@Component({
  selector: 'app-login',
  imports: [ReactiveFormsModule, LucideDynamicIcon],
  templateUrl: './login.html',
  styleUrl: './login.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Login {
  private readonly fb = inject(NonNullableFormBuilder);
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);
  private readonly connectedUserStore = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public isPending = signal<boolean>(false);
  public errorIcon = LucideCircleAlert;

  public readonly loginForm: FormGroup<LoginForm> = this.fb.group({
    handle: ['', [Validators.required, Validators.pattern(/^@?[^@\s]+@[^@\s]+$/)]],
    nsec: ['', [Validators.required]],
  });

  public onSubmit(): void {
    if (this.loginForm.invalid) {
      this.loginForm.markAllAsTouched();
      return;
    }
    this.isPending.set(true);
    this.authService
      .login(this.loginForm.getRawValue())
      .pipe(finalize(() => this.isPending.set(false)))
      .subscribe({
        next: (user: ConnectedUser) => {
          this.connectedUserStore.setLoggedInUser(user);
          this.modalService.close();
          this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
        },
        error: (err: CmdError) => {
          const { handle, nsec } = this.loginForm.controls;
          switch (err.code) {
            case 'not_found':
              handle.setErrors({ notFound: true });
              break;
            case 'invalid':
              nsec.setErrors({ invalid: true });
              break;
            case 'key_mismatch':
              this.loginForm.setErrors({ mismatch: true });
              break;
            default:
              this.loginForm.setErrors({ unknown: true });
          }
        },
      });
  }
}
