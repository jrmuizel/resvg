// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use crate::ScreenSize;


type LayerData<T> = Rc<RefCell<T>>;

/// Stack of image layers.
///
/// Instead of creating a new layer each time we need one,
/// we are reusing an existing one.
pub struct Layers<T> {
    d: Vec<LayerData<T>>,
    /// Use Rc as a shared counter.
    counter: Rc<()>,
    img_size: ScreenSize,
    dpi: f64,
    new_img_fn: Box<Fn(ScreenSize, f64) -> Option<T>>,
    clear_img_fn: Box<Fn(&mut T)>,
}

impl<T> Layers<T> {
    /// Creates `Layers`.
    pub fn new<F1, F2>(
        img_size: ScreenSize,
        dpi: f64,
        new_img_fn: F1,
        clear_img_fn: F2,
    ) -> Self
    where
        F1: Fn(ScreenSize, f64) -> Option<T> + 'static,
        F2: Fn(&mut T) + 'static,
    {
        Layers {
            d: Vec::new(),
            counter: Rc::new(()),
            img_size,
            dpi,
            new_img_fn: Box::new(new_img_fn),
            clear_img_fn: Box::new(clear_img_fn),
        }
    }

    /// Returns a layer size.
    pub fn image_size(&self) -> ScreenSize {
        self.img_size
    }

    /// Returns a first free layer to draw on.
    ///
    /// - If there are no free layers - will create a new one.
    /// - If there is a free layer - it will clear it before return.
    pub fn get(&mut self) -> Option<Layer<T>> {
        let used_layers = Rc::strong_count(&self.counter) - 1;
        if used_layers == self.d.len() {
            match (self.new_img_fn)(self.img_size, self.dpi) {
                Some(img) => {
                    self.d.push(Rc::new(RefCell::new(img)));
                    Some(Layer {
                        d: self.d[self.d.len() - 1].clone(),
                        _counter_holder: self.counter.clone(),
                    })
                }
                None => {
                    None
                }
            }
        } else {
            {
                let img = self.d[used_layers].clone();
                (self.clear_img_fn)(&mut img.borrow_mut());
            }

            Some(Layer {
                d: self.d[used_layers].clone(),
                _counter_holder: self.counter.clone(),
            })
        }
    }
}

impl<T> Drop for Layers<T> {
    fn drop(&mut self) {
        debug_assert!(Rc::strong_count(&self.counter) == 1);
    }
}

/// The layer object.
pub struct Layer<T> {
    d: LayerData<T>,
    // When Layer goes out of scope, Layers::counter will be automatically decreased.
    _counter_holder: Rc<()>,
}

impl<T> Deref for Layer<T> {
    type Target = LayerData<T>;

    fn deref(&self) -> &Self::Target {
        &self.d
    }
}
