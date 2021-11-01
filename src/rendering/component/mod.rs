use crate::layout::{ComponentState, LayoutState};

use super::{
    consts::{DEFAULT_COMPONENT_HEIGHT, PSEUDO_PIXELS, SEPARATOR_THICKNESS, TWO_ROW_HEIGHT},
    resource::ResourceAllocator,
    RenderContext,
};

pub mod blank_space;
pub mod detailed_timer;
pub mod graph;
pub mod key_value;
pub mod separator;
pub mod splits;
pub mod text;
pub mod timer;
pub mod title;

pub enum Cache<I> {
    DetailedTimer(detailed_timer::Cache<I>),
    Splits(splits::Cache<I>),
    Title(title::Cache<I>),
    Empty,
}

macro_rules! accessors {
    ($($variant:ident $module:ident),*) => {
        $(
            fn $module(&mut self) -> &mut $module::Cache<I> {
                match self {
                    Self::$variant(c) => c,
                    _ => {
                        *self = Self::$variant($module::Cache::new());
                        self.$module()
                    }
                }
            }
        )*
    };
}

impl<I> Cache<I> {
    pub const fn new(component: &ComponentState) -> Self {
        match component {
            ComponentState::DetailedTimer(_) => Self::DetailedTimer(detailed_timer::Cache::new()),
            ComponentState::Splits(_) => Self::Splits(splits::Cache::new()),
            ComponentState::Title(_) => Self::Title(title::Cache::new()),
            _ => Self::Empty,
        }
    }

    fn make_empty(&mut self) {
        *self = Self::Empty;
    }

    accessors! {
        DetailedTimer detailed_timer,
        Splits splits,
        Title title
    }
}

pub fn layout_width(layout: &LayoutState) -> f32 {
    layout.components.iter().map(width).sum()
}

pub fn layout_height(layout: &LayoutState) -> f32 {
    layout.components.iter().map(height).sum()
}

pub fn width(component: &ComponentState) -> f32 {
    match component {
        ComponentState::BlankSpace(state) => state.size as f32 * PSEUDO_PIXELS,
        ComponentState::DetailedTimer(_) => 7.0,
        ComponentState::Graph(_) => 7.0,
        ComponentState::KeyValue(_) => 6.0,
        ComponentState::Separator(_) => SEPARATOR_THICKNESS,
        ComponentState::Splits(state) => {
            let column_count = 2.0; // FIXME: Not always 2.
            let split_width = 2.0 + column_count * splits::COLUMN_WIDTH;
            state.splits.len() as f32 * split_width
        }
        ComponentState::Text(_) => 6.0,
        ComponentState::Timer(_) => 8.25,
        ComponentState::Title(_) => 8.0,
    }
}

pub fn height(component: &ComponentState) -> f32 {
    match component {
        ComponentState::BlankSpace(state) => state.size as f32 * PSEUDO_PIXELS,
        ComponentState::DetailedTimer(_) => 2.5,
        ComponentState::Graph(state) => state.height as f32 * PSEUDO_PIXELS,
        ComponentState::KeyValue(state) => {
            if state.display_two_rows {
                TWO_ROW_HEIGHT
            } else {
                DEFAULT_COMPONENT_HEIGHT
            }
        }
        ComponentState::Separator(_) => SEPARATOR_THICKNESS,
        ComponentState::Splits(state) => {
            state.splits.len() as f32
                * if state.display_two_rows {
                    TWO_ROW_HEIGHT
                } else {
                    DEFAULT_COMPONENT_HEIGHT
                }
                + if state.column_labels.is_some() {
                    DEFAULT_COMPONENT_HEIGHT
                } else {
                    0.0
                }
        }
        ComponentState::Text(state) => {
            if state.display_two_rows {
                TWO_ROW_HEIGHT
            } else {
                DEFAULT_COMPONENT_HEIGHT
            }
        }
        ComponentState::Timer(state) => state.height as f32 * PSEUDO_PIXELS,
        ComponentState::Title(_) => TWO_ROW_HEIGHT,
    }
}

pub(super) fn render<A: ResourceAllocator>(
    cache: &mut Cache<A::Image>,
    context: &mut RenderContext<'_, A>,
    component: &ComponentState,
    state: &LayoutState,
    dim: [f32; 2],
) {
    match component {
        ComponentState::BlankSpace(state) => {
            cache.make_empty();
            blank_space::render(context, dim, state)
        }
        ComponentState::DetailedTimer(component) => {
            detailed_timer::render(cache.detailed_timer(), context, dim, component, state)
        }
        ComponentState::Graph(component) => {
            cache.make_empty();
            graph::render(context, dim, component, state)
        }
        ComponentState::KeyValue(component) => {
            cache.make_empty();
            key_value::render(context, dim, component, state)
        }
        ComponentState::Separator(component) => {
            cache.make_empty();
            separator::render(context, dim, component, state)
        }
        ComponentState::Splits(component) => {
            splits::render(cache.splits(), context, dim, component, state)
        }
        ComponentState::Text(component) => {
            cache.make_empty();
            text::render(context, dim, component, state)
        }
        ComponentState::Timer(component) => {
            cache.make_empty();
            timer::render(context, dim, component);
        }
        ComponentState::Title(component) => {
            title::render(cache.title(), context, dim, component, state)
        }
    }
}
