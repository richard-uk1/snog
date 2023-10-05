use rand::Rng;
use snog::{
    kurbo::{Affine, Line, Size, Stroke},
    peniko::{Brush, Color},
    App, AppLogic, RenderCtx,
};

fn main() {
    let data = Data::new();
    App::new_with_data(data).run()
}

struct Data {
    lines: Vec<(Line, Color)>,
}

impl Data {
    fn new() -> Self {
        const NUM: usize = 1_000;
        let mut lines = Vec::with_capacity(NUM);
        let mut rng = rand::thread_rng();
        for _ in 0..NUM {
            let line = Line::new((rng.gen(), rng.gen()), (rng.gen(), rng.gen()));
            let color = Color::hlca(
                rng.gen::<f64>() * 360.,
                rng.gen::<f64>() * 100.,
                rng.gen::<f64>() * 127.,
                1.,
            );
            lines.push((line, color));
        }
        Data { lines }
    }
}

impl AppLogic for Data {
    fn render(&mut self, ctx: &mut RenderCtx) {
        let Size { width, height } = ctx.screen().size();

        let stroke = Stroke::new(2.);
        let scale = Affine::scale_non_uniform(width, height);
        for (line, color) in self.lines.iter().copied() {
            let brush = Brush::Solid(color);
            ctx.stroke(&stroke, Affine::IDENTITY, &brush, None, &(scale * line));
        }
    }
}
