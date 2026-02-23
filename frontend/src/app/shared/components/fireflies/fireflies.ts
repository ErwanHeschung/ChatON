import {
  Component,
  ElementRef,
  OnInit,
  ViewChild,
  OnDestroy,
  input,
  ChangeDetectionStrategy,
} from '@angular/core';

type Firefly = {
  x: number;
  y: number;
  s: number;
  op: number;
  maxOp: number;
  fadeSpeed: number;
  rising: boolean;
  speedX: number;
  speedY: number;
};

@Component({
  selector: 'app-fireflies',
  standalone: true,
  templateUrl: './fireflies.html',
  styleUrl: './fireflies.scss',
  host: {
    '(window:resize)': 'onResize()',
  },
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class FirefliesComponent implements OnInit, OnDestroy {
  @ViewChild('fireflyCanvas', { static: true })
  private readonly canvasRef!: ElementRef<HTMLCanvasElement>;

  public count = input<number>(50);
  public color = input<string>('34, 197, 94');

  private ctx!: CanvasRenderingContext2D;
  private readonly fireflies: Firefly[] = [];
  private animationId: number = 0;

  public onResize(): void {
    this.setupCanvasDimensions();
  }

  public ngOnInit(): void {
    const context = this.canvasRef.nativeElement.getContext('2d');
    if (!context) throw new Error('Could not get Canvas context');

    this.ctx = context;
    this.setupCanvasDimensions();

    this.generateFireflies(this.count());

    this.animate();
  }

  private setupCanvasDimensions(): void {
    this.canvasRef.nativeElement.width = window.innerWidth;
    this.canvasRef.nativeElement.height = window.innerHeight;
  }

  private generateFireflies(count: number): void {
    for (let i = 0; i < count; i++) {
      this.fireflies.push(this.createNewFirefly());
    }
  }

  private createNewFirefly(): Firefly {
    return {
      x: Math.random() * window.innerWidth,
      y: Math.random() * window.innerHeight,
      s: Math.random() * 3 + 1,
      op: 0,
      maxOp: Math.random() * 0.6 + 0.2,
      fadeSpeed: Math.random() * 0.005,
      rising: true,
      speedX: (Math.random() - 0.5) * 0.2,
      speedY: (Math.random() - 0.5) * 0.2,
    };
  }

  private animate(): void {
    this.ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);

    for (const f of this.fireflies) {
      f.x += f.speedX;
      f.y += f.speedY;

      if (f.rising) {
        f.op += f.fadeSpeed;
        if (f.op >= f.maxOp) f.rising = false;
      } else {
        f.op -= f.fadeSpeed;
        if (f.op <= 0) {
          Object.assign(f, this.createNewFirefly());
        }
      }

      this.draw(f);
    }

    this.animationId = requestAnimationFrame(() => this.animate());
  }

  private draw(f: Firefly): void {
    this.ctx.beginPath();
    this.ctx.arc(f.x, f.y, f.s, 0, Math.PI * 2);
    this.ctx.fillStyle = `rgba(${this.color()}, ${f.op})`;
    this.ctx.shadowBlur = 8;
    this.ctx.shadowColor = `rgb(${this.color()})`;
    this.ctx.fill();
    this.ctx.closePath();
  }

  public ngOnDestroy(): void {
    if (this.animationId) {
      cancelAnimationFrame(this.animationId);
    }
  }
}
