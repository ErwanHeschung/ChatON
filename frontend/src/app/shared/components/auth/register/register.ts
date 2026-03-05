import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';
import { Router } from '@angular/router';
import { ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { CircleAlert, LucideAngularModule } from 'lucide-angular';
import { finalize } from 'rxjs';
import { ROUTES } from 'src/app/app.routes';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';

interface RegisterForm {
  username: FormControl<string>;
  email: FormControl<string>;
  password: FormControl<string>;
  confirmPassword: FormControl<string>;
}

@Component({
  selector: 'app-register',
  imports: [ReactiveFormsModule, LucideAngularModule],
  templateUrl: './register.html',
  styleUrl: './register.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Register {
  public isPending = signal<boolean>(false);
  public errorIcon = CircleAlert;

  private readonly router = inject(Router);
  private readonly fb = inject(NonNullableFormBuilder);
  private readonly authService = inject(AuthService);
  private readonly connectedUserStore = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public readonly registerForm: FormGroup<RegisterForm> = this.fb.group(
    {
      username: ['', [Validators.required, Validators.minLength(3)]],
      email: ['', [Validators.required, Validators.email]],
      password: ['', [Validators.required, Validators.minLength(8)]],
      confirmPassword: ['', [Validators.required]],
    },
    {
      validators: (group) => {
        const pass = group.get('password')?.value;
        const confirm = group.get('confirmPassword')?.value;
        return pass === confirm ? null : { mismatch: true };
      },
    },
  );

  public onSubmit(): void {
    this.isPending.set(true);
    if (this.registerForm.valid) {
      this.authService
        .register(this.registerForm.getRawValue())
        .pipe(finalize(() => this.isPending.set(false)))
        .subscribe({
          next: (user: ConnectedUser) => {
            this.connectedUserStore.setLoggedInUser(user);
            this.modalService.close();
            this.router.navigate([ROUTES.CHAT]);
          },
          error: (err) => {
            if (err.status === 409) {
              this.registerForm.controls.username.setErrors({ conflict: true });
            } else {
              this.registerForm.setErrors({ unknown: true });
            }
          },
        });
    } else {
      this.registerForm.markAllAsTouched();
    }
  }
}
