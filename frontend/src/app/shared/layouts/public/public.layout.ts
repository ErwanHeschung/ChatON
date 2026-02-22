import { Component } from '@angular/core';
import { FirefliesComponent } from '../../components/fireflies/fireflies';

@Component({
 selector: 'app-public',
 imports: [FirefliesComponent],
 templateUrl: './public.layout.html',
 styleUrl: './public.layout.scss',
})
export class Public {}
