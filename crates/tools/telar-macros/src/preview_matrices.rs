//! The named matrices and viewports of `[telar.previews]`, installed as the binary loads so `Matrix::Named` resolves against them in whatever runs a preview: the workshop, the test runner, a snapshot or a hot-reload dylib.

use proc_macro2::{Literal, TokenStream as TokenStream2};
use quote::quote;
use telar_project::{MatrixAxis, MatrixControlSize, MatrixDirection, PreviewsSection};

use crate::Invocation;

/// The constructor that installs `previews`' matrices and viewports, or nothing when it names neither. `app!` installs them as the application's; `rsx_modules!` only fills an empty slot, as it does for the catalog.
pub(crate) fn matrices_install(
    previews: &PreviewsSection,
    invocation: Invocation,
) -> Result<TokenStream2, String> {
    let matrices = previews
        .named_matrices()
        .map_err(|problems| problems.join("; "))?;
    let viewports = previews.viewports.clone().unwrap_or_default();
    if matrices.is_empty() && viewports.is_empty() {
        return Ok(TokenStream2::new());
    }
    let matrices = matrices.iter().map(|(name, axes)| {
        let axes = axes.iter().map(axis_tokens);
        quote! { ::telar::preview::NamedMatrix::new(#name, &[#(#axes),*]) }
    });
    let viewports = viewports.iter().map(|(name, size)| {
        let width = Literal::f32_suffixed(size.width as f32);
        let height = Literal::f32_suffixed(size.height as f32);
        quote! { ::telar::preview::ViewportPreset::new(#name, #width, #height) }
    });
    let install = match invocation {
        Invocation::App => quote! { ::telar::preview::__install_matrices },
        Invocation::Modules => quote! { ::telar::preview::__install_matrices_if_unset },
    };
    Ok(quote! {
        ::telar::__previews! {
            ::telar::__ctor::declarative::ctor! {
                #[ctor(unsafe, anonymous)]
                fn install_preview_matrices() {
                    static MATRICES: &[::telar::preview::NamedMatrix] = &[#(#matrices),*];
                    static VIEWPORTS: &[::telar::preview::ViewportPreset] = &[#(#viewports),*];
                    #install(MATRICES, VIEWPORTS);
                }
            }
        }
    })
}

fn axis_tokens(axis: &MatrixAxis) -> TokenStream2 {
    let axis_path = quote! { ::telar::preview::Axis };
    match axis {
        MatrixAxis::Mode(modes) => quote! { #axis_path::Mode(&[#(#modes),*]) },
        MatrixAxis::Locale(locales) => quote! { #axis_path::Locale(&[#(#locales),*]) },
        MatrixAxis::Dir(directions) => {
            let directions = directions.iter().map(|direction| match direction {
                MatrixDirection::Ltr => quote! { ::telar::Direction::Ltr },
                MatrixDirection::Rtl => quote! { ::telar::Direction::Rtl },
            });
            quote! { #axis_path::Dir(&[#(#directions),*]) }
        }
        MatrixAxis::Viewport(viewports) => quote! { #axis_path::Viewport(&[#(#viewports),*]) },
        MatrixAxis::ControlSize(sizes) => {
            let sizes = sizes.iter().map(|size| match size {
                MatrixControlSize::Mini => quote! { ::telar::ControlSize::Mini },
                MatrixControlSize::Small => quote! { ::telar::ControlSize::Small },
                MatrixControlSize::Regular => quote! { ::telar::ControlSize::Regular },
                MatrixControlSize::Large => quote! { ::telar::ControlSize::Large },
            });
            quote! { #axis_path::ControlSize(&[#(#sizes),*]) }
        }
        MatrixAxis::Arg { name, values } => quote! { #axis_path::Arg(#name, &[#(#values),*]) },
    }
}

#[cfg(test)]
#[path = "preview_matrices_test.rs"]
mod tests;
