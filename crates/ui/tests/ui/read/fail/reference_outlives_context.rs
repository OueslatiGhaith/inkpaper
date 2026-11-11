use core::mem::MaybeUninit;

use inkpaper_ui::prelude::*;

struct Model {
    name: &'static str,
}

struct Other;

fn main() {
    let memory = Box::leak(vec![MaybeUninit::uninit(); 4096].into_boxed_slice());
    let runtime = Runtime::<()>::new(Pool::new(memory));
    let model = runtime.create(|_| Model { name: "hello" }).unwrap();
    let other = runtime.create(|_| Other).unwrap();

    // the entity is borrowed only while the context lives
    let model = runtime.update(other, |_, cx| model.read(cx)).unwrap();

    let _ = model.name;
}
