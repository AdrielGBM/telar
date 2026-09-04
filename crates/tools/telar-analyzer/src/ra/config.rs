//! How the embedded rust-analyzer is configured.

use ra_ap_hir::ClosureStyle;
use ra_ap_ide::{
    AdjustmentHints, AdjustmentHintsMode, ClosureReturnTypeHints, CompletionConfig,
    CompletionFieldsToResolve, DiagnosticsConfig, DiscriminantHints, FindAllRefsConfig,
    GenericParameterHints, GotoDefinitionConfig, HoverConfig, HoverDocFormat, InlayFieldsToResolve,
    InlayHintsConfig, LifetimeElisionHints, RaFixtureConfig, SubstTyLen, TypeHintsPlacement,
};
use ra_ap_ide_db::imports::insert_use::{ImportGranularity, InsertUseConfig, PrefixKind};

/// A conservative completion config: no fly-imports / snippets / term-search, so the sub-config surface (and version fragility) stays minimal.
pub(super) fn completion_config() -> CompletionConfig<'static> {
    CompletionConfig {
        enable_postfix_completions: true,
        enable_imports_on_the_fly: false,
        enable_self_on_the_fly: true,
        enable_auto_iter: false,
        enable_auto_await: false,
        enable_private_editable: false,
        enable_term_search: false,
        term_search_fuel: 0,
        full_function_signatures: false,
        callable: None,
        add_colons_to_module: true,
        add_semicolon_to_unit: false,
        snippet_cap: None,
        insert_use: insert_use_config(),
        prefer_no_std: false,
        prefer_prelude: true,
        prefer_absolute: false,
        snippets: Vec::new(),
        limit: None,
        fields_to_resolve: CompletionFieldsToResolve::empty(),
        exclude_flyimport: Vec::new(),
        exclude_traits: &[],
        ra_fixture: RaFixtureConfig::default(),
    }
}

/// Shared import config for completion + diagnostics; both need the same (conservative) settings.
fn insert_use_config() -> InsertUseConfig {
    InsertUseConfig {
        granularity: ImportGranularity::Crate,
        enforce_granularity: false,
        prefix_kind: PrefixKind::Plain,
        group: false,
        skip_glob_imports: false,
    }
}

/// A conservative hover config: markdown on, doc links off (the client can't resolve rust-analyzer's generated-file URLs), no memory-layout block, no field/variant caps.
pub(super) fn hover_config() -> HoverConfig<'static> {
    HoverConfig {
        links_in_hover: false,
        memory_layout: None,
        documentation: true,
        keywords: true,
        format: HoverDocFormat::Markdown,
        max_trait_assoc_items_count: None,
        max_fields_count: None,
        max_enum_variants_count: None,
        max_subst_ty_len: SubstTyLen::Unlimited,
        show_drop_glue: false,
        ra_fixture: RaFixtureConfig::default(),
    }
}

pub(super) fn goto_definition_config() -> GotoDefinitionConfig<'static> {
    GotoDefinitionConfig {
        ra_fixture: RaFixtureConfig::default(),
    }
}

/// Workspace-wide find-all-references: no scope limit (so cross-file Rust refs to a component's generated `fn`/`Props` are found), imports + tests included (the generated modules are neither, and the backend filters generated build files itself).
pub(super) fn find_all_refs_config() -> FindAllRefsConfig<'static> {
    FindAllRefsConfig {
        search_scope: None,
        ra_fixture: RaFixtureConfig::default(),
        exclude_imports: false,
        exclude_tests: false,
    }
}

/// Inlay-hints config: type + parameter hints only (the rest off), mirroring rust-analyzer's own disabled baseline so the large config surface stays a faithful copy rather than a guess. The backend keeps only the `[logic]`-origin hints.
pub(super) fn inlay_hints_config() -> InlayHintsConfig<'static> {
    InlayHintsConfig {
        render_colons: true,
        type_hints: true,
        type_hints_placement: TypeHintsPlacement::Inline,
        sized_bound: false,
        discriminant_hints: DiscriminantHints::Never,
        parameter_hints: true,
        parameter_hints_for_missing_arguments: false,
        generic_parameter_hints: GenericParameterHints {
            type_hints: false,
            lifetime_hints: false,
            const_hints: false,
        },
        chaining_hints: false,
        adjustment_hints: AdjustmentHints::Never,
        adjustment_hints_disable_reborrows: false,
        adjustment_hints_mode: AdjustmentHintsMode::Prefix,
        adjustment_hints_hide_outside_unsafe: false,
        closure_return_type_hints: ClosureReturnTypeHints::Never,
        closure_capture_hints: false,
        binding_mode_hints: false,
        implicit_drop_hints: false,
        implied_dyn_trait_hints: false,
        lifetime_elision_hints: LifetimeElisionHints::Never,
        param_names_for_lifetime_elision_hints: false,
        hide_inferred_type_hints: false,
        hide_named_constructor_hints: false,
        hide_closure_initialization_hints: false,
        hide_closure_parameter_hints: false,
        range_exclusive_hints: false,
        closure_style: ClosureStyle::ImplFn,
        max_length: Some(25),
        closing_brace_hints_min_lines: None,
        fields_to_resolve: InlayFieldsToResolve::empty(),
        ra_fixture: RaFixtureConfig::default(),
    }
}

/// Diagnostics config: proc macros on (the `app!`-generated modules are only visible through expansion). Experimental diagnostics stay OFF: enabling them to catch unresolved names was tried and reverted — in the tightly-coupled generated `[view]` a single broken reference (a typo'd helper, or a `tex` tag → missing `tex(ctx)`) makes the whole builder expression error-typed and rust-analyzer emits `E0425` ("no such value") for dozens of sibling identifiers, flooding the `.rsx` (the stock rust-analyzer reports the same few *real* errors on the generated `.rs`; ours cascaded). Filtering by code didn't help — the flood is `E0425`-shaped. Syntax + stable name-resolution still surface; richer semantic errors are left to the stock analyzer / a future `cargo check` flycheck. No `Default` impl.
pub(super) fn diagnostics_config() -> DiagnosticsConfig {
    DiagnosticsConfig {
        enabled: true,
        proc_macros_enabled: true,
        proc_attr_macros_enabled: true,
        disable_experimental: true,
        disabled: Default::default(),
        expr_fill_default: Default::default(),
        style_lints: false,
        snippet_cap: None,
        insert_use: insert_use_config(),
        prefer_no_std: false,
        prefer_prelude: true,
        prefer_absolute: false,
        term_search_fuel: 0,
        show_rename_conflicts: false,
    }
}
