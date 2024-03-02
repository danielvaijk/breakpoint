use anyhow::{bail, Result};
use std::path::Path;
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::SourceMap;
use swc_ecma_ast::{
    Decl, EsVersion, ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, Module,
    NamedExport,
};
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::{Parser, Syntax};

pub fn parse_esm_module(file_path: &Path) -> Result<Module> {
    let is_ts_file = match file_path.extension().unwrap().to_str().unwrap() {
        "ts" | "tsx" => true,
        _ => false,
    };

    let source_map: Lrc<SourceMap> = Default::default();
    let source_file = source_map.load_file(file_path)?;

    let syntax = if is_ts_file {
        Syntax::Typescript(Default::default())
    } else {
        Syntax::Es(Default::default())
    };

    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::from(&*source_file),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(source_map));

    for parse_error in parser.take_errors() {
        parse_error.into_diagnostic(&handler).emit();
    }

    let module = if is_ts_file {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    if let Err(parse_error) = module {
        parse_error.into_diagnostic(&handler).emit();
        bail!("Failed to parse module '{}'.", file_path.display());
    }

    Ok(module.unwrap())
}

pub fn get_items_in_module(
    module: &Module,
) -> (
    Vec<&Decl>,
    Vec<&ExportDecl>,
    Vec<&ExportAll>,
    Vec<&NamedExport>,
    Option<&ExportDefaultDecl>,
    Option<&ExportDefaultExpr>,
) {
    let mut declarations: Vec<&Decl> = Vec::new();
    let mut declarations_with_export: Vec<&ExportDecl> = Vec::new();

    let mut exports_facade: Vec<&ExportAll> = Vec::new();
    let mut exports_named: Vec<&NamedExport> = Vec::new();

    let mut default_export_declaration: Option<&ExportDefaultDecl> = None;
    let mut default_export_expression: Option<&ExportDefaultExpr> = None;

    for item in module.body.iter() {
        if item.is_stmt() {
            let statement = item.as_stmt().unwrap();

            if statement.is_decl() {
                declarations.push(statement.as_decl().unwrap());
            }
        } else if item.is_module_decl() {
            let module_declaration = item.as_module_decl().unwrap();

            if module_declaration.is_export_decl() {
                declarations_with_export.push(module_declaration.as_export_decl().unwrap());
            } else if module_declaration.is_export_default_decl() {
                default_export_declaration = module_declaration.as_export_default_decl();
            } else if module_declaration.is_export_default_expr() {
                default_export_expression = module_declaration.as_export_default_expr();
            } else if module_declaration.is_export_all() {
                exports_facade.push(module_declaration.as_export_all().unwrap());
            } else if module_declaration.is_export_named() {
                exports_named.push(module_declaration.as_export_named().unwrap());
            }
        }
    }

    (
        declarations,
        declarations_with_export,
        exports_facade,
        exports_named,
        default_export_declaration,
        default_export_expression,
    )
}
