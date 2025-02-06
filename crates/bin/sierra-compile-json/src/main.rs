use std::fs;
use std::path::PathBuf;
use anyhow::Context;
use cairo_lang_sierra_to_casm::compiler::{CasmCairoProgram, SierraToCasmConfig, compile};
use cairo_lang_sierra_to_casm::metadata::{calc_metadata, calc_metadata_ap_change_only};
use clap::Parser;
use cairo_lang_compiler::{
    compile_prepared_db, db::RootDatabase, project::setup_project, CompilerConfig, compile_cairo_project_at_path
};
/// Compiles a Sierra file (Cairo Program) into serialized CASM.
/// Exits with 0/1 if the compilation succeeds/fails.
#[derive(Parser, Debug)]
#[clap(version, verbatim_doc_comment)]
struct Args {
    /// The path of the file to compile.
    path: PathBuf,
    /// The output file name (default: stdout).
    output: Option<String>,
    /// Compile with/without gas
    #[arg(long, default_value_t = false)]
    include_gas: bool,
    /// Add gas usage check
    #[arg(long, default_value_t = false)]
    gas_usage_check: bool,
}

fn main() -> anyhow::Result<()> {
    
    let args = Args::parse();
    let casm_cairo_program: CasmCairoProgram;
    if args.include_gas {
        let sierra_program = compile_cairo_project_at_path(&args.path, CompilerConfig {
            replace_ids: true,
            inlining_strategy: cairo_lang_lowering::utils::InliningStrategy::Default,
            ..CompilerConfig::default()
        })?;
    
        let sierra_to_casm_config =
            SierraToCasmConfig { gas_usage_check: args.gas_usage_check, max_bytecode_size: usize::MAX };
    
        let cairo_program = compile(
            &sierra_program,
            &calc_metadata(&sierra_program, Default::default())
                .with_context(|| "Failed calculating Sierra variables.")?,
            sierra_to_casm_config,
        )
        .with_context(|| "Compilation failed.")?;
    
        casm_cairo_program = CasmCairoProgram::new(&sierra_program, &cairo_program)
            .with_context(|| "Sierra to Casm compilation failed.")?;
    } else {
        let file = std::fs::read(&args.path)?;
        let sierra_program = match serde_json::from_slice(&file) {
            Ok(program) => program,
            Err(_) => {
                // If it fails, try to compile it as a cairo program
                let compiler_config = CompilerConfig {
                    replace_ids: true,
                    ..CompilerConfig::default()
                };
                let mut db = RootDatabase::builder()
                    .detect_corelib()
                    .skip_auto_withdraw_gas()
                    .build()
                    .unwrap();
                let main_crate_ids = setup_project(&mut db, &args.path).unwrap();
                let sierra_program_with_dbg =
                    compile_prepared_db(&db, main_crate_ids, compiler_config).unwrap();

                sierra_program_with_dbg.program
            }
        };
        let config = SierraToCasmConfig {
            gas_usage_check: false,
            max_bytecode_size: usize::MAX,
        };
        let metadata = calc_metadata_ap_change_only(&sierra_program)
            .with_context(|| "Failed calculating metadata.")?;
        let cairo_program = compile(&sierra_program, &metadata, config)?;
        casm_cairo_program = CasmCairoProgram::new(&sierra_program, &cairo_program).with_context(|| "Sierra to Casm compilation failed.")?;
    }

    let res = serde_json::to_string(&casm_cairo_program)
        .with_context(|| "Casm contract Serialization failed.")?;

    match args.output {
        Some(path) => fs::write(path, res).with_context(|| "Failed to write casm contract.")?,
        None => println!("{res}"),
    }
    Ok(())
}

