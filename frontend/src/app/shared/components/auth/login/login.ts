import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';

type RegisterForm = {
  username: FormControl<string>;
  password: FormControl<string>;
};

@Component({
  selector: 'app-login',
  imports: [ReactiveFormsModule],
  templateUrl: './login.html',
  styleUrl: './login.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Login {
  private readonly fb = inject(NonNullableFormBuilder);

  public readonly LoginForm: FormGroup<RegisterForm> = this.fb.group({
    username: ['', [Validators.required, Validators.minLength(3)]],
    password: ['', [Validators.required, Validators.minLength(8)]],
  });

  public onSubmit(): void {
    if (this.LoginForm.valid) {
      const rawValue = this.LoginForm.getRawValue();
      console.log('Connexion de :', rawValue.username);
    } else {
      this.LoginForm.markAllAsTouched();
    }
  }
}
