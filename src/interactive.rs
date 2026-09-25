//! Interactive prompts for `estimate --interactive`.
//!
//! Typing `--arg key=val` for contracts with several parameters is
//! error-prone, so interactive mode walks the user through the invocation:
//! it lists the contract's exported functions, prompts for a selection when
//! `--fn` was not given, then prompts for each parameter showing its spec
//! name and type (e.g. `Enter step (i64): `).
//!
//! Explicit `--fn`/`--arg`/`--id` flags always take precedence — the prompt
//! only fills in the gaps, so a fully-specified invocation never blocks on
//! stdin. Every answer is validated against the contract-spec type before
//! proceeding (invalid input re-prompts); EOF on stdin (Ctrl-D, closed pipe)
//! aborts with a clear error instead of hanging or panicking. Ctrl-C aborts
//! via the default SIGINT disposition, which terminates the process without
//! unwinding through the prompt code.

use std::io::{BufRead, Write};

use crate::error::{AppError, AppResult};
use crate::wasm::parser::{FunctionInfo, ParamInfo, format_function, validate_arg_value};
use crate::xdr_helper;

/// Everything `estimate` needs that the CLI flags did not already provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractiveSelection {
    /// Function to invoke (always resolved in interactive mode).
    pub function: String,
    /// Arguments as `key=value` strings, in parameter declaration order.
    pub args: Vec<String>,
    /// Contract ID to invoke (the `--id` flag value, or the prompted one).
    pub contract_id: Option<String>,
}

/// Resolve the invocation by prompting on `reader`/`writer`.
///
/// * `functions` - Exported functions enumerated from the WASM.
/// * `fn_name` - Pre-selected function (`--fn`); skips the selection prompt.
/// * `existing_args` - Pre-supplied `--arg` entries; matched by `key=` name
///   and kept, so only missing parameters are prompted for.
/// * `contract_id` - Pre-supplied `--id`; skips the contract-ID prompt.
///
/// # Network calls
/// None — pure terminal I/O.
pub fn prompt_for_invocation(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    functions: &[FunctionInfo],
    fn_name: Option<&str>,
    existing_args: &[String],
    contract_id: Option<&str>,
) -> AppResult<InteractiveSelection> {
    let fn_info: &FunctionInfo = match fn_name {
        Some(name) => match functions.iter().find(|f| f.name == name) {
            Some(info) => info,
            None => {
                let available = functions
                    .iter()
                    .map(|f| f.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(AppError::General(format!(
                    "unknown function '{name}' (available: {available})"
                )));
            }
        },
        None => {
            if functions.is_empty() {
                return Err(AppError::General(
                    "interactive mode needs at least one exported function; pass --fn explicitly or estimate a WASM upload without --interactive"
                        .to_string(),
                ));
            }
            let idx = select_function(reader, writer, functions)?;
            &functions[idx]
        }
    };

    let args = if fn_info.params.is_empty() {
        if fn_info.param_count == 0 {
            existing_args.to_vec()
        } else {
            collect_untyped_args(reader, writer, fn_info.param_count, existing_args)?
        }
    } else {
        collect_typed_args(reader, writer, &fn_info.params, existing_args)?
    };
    let contract_id = prompt_contract_id(reader, writer, contract_id)?;

    Ok(InteractiveSelection {
        function: fn_info.name.clone(),
        args,
        contract_id,
    })
}

/// Print the numbered function list and read a selection.
///
/// Accepts a 1-based number or an exact function name; anything else
/// re-prompts. Returns the index into `functions`.
fn select_function(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    functions: &[FunctionInfo],
) -> AppResult<usize> {
    writeln!(writer, "Available functions:")?;
    for (idx, fn_info) in functions.iter().enumerate() {
        writeln!(writer, "  {}. {}", idx + 1, format_function(fn_info))?;
    }
    loop {
        let input = read_prompt(
            reader,
            writer,
            &format!("Select a function [1-{} or name]: ", functions.len()),
        )?;
        if let Ok(n) = input.parse::<usize>() {
            if (1..=functions.len()).contains(&n) {
                return Ok(n - 1);
            }
        }
        if let Some(idx) = functions.iter().position(|f| f.name == input) {
            return Ok(idx);
        }
        writeln!(
            writer,
            "Unknown selection '{input}' — enter a number 1-{} or an exact function name.",
            functions.len()
        )?;
    }
}

/// Collect typed arguments in parameter declaration order.
///
/// Entries already supplied via `--arg` (matched by `key=`) are kept as-is;
/// every other parameter is prompted for as `Enter <name> (<type>): ` and
/// validated against its spec type, re-prompting on mismatch. Leftover
/// entries that matched no parameter are appended untouched so the
/// downstream arity check reports them instead of silently dropping them.
fn collect_typed_args(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    params: &[ParamInfo],
    existing_args: &[String],
) -> AppResult<Vec<String>> {
    let mut collected = Vec::with_capacity(params.len());
    let mut used = vec![false; existing_args.len()];
    for param in params {
        if let Some((idx, entry)) = existing_args
            .iter()
            .enumerate()
            .find(|item| !used[item.0] && arg_key(item.1) == Some(param.name.as_str()))
        {
            used[idx] = true;
            collected.push(entry.clone());
        } else {
            loop {
                let value = read_prompt(
                    reader,
                    writer,
                    &format!("Enter {} ({}): ", param.name, param.type_name),
                )?;
                match validate_arg_value(&param.type_def, &value) {
                    Ok(()) => {
                        collected.push(format!("{}={value}", param.name));
                        break;
                    }
                    Err(e) => {
                        writeln!(writer, "Invalid value: {e}. Try again.")?;
                    }
                }
            }
        }
    }
    for (idx, entry) in existing_args.iter().enumerate() {
        if !used[idx] {
            collected.push(entry.clone());
        }
    }
    Ok(collected)
}

/// Collect arguments for a function without spec types (bare WASM exports).
///
/// Nothing can be validated, so pre-supplied `--arg` entries are used as-is
/// and otherwise one bare value per parameter is prompted for.
fn collect_untyped_args(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    param_count: u32,
    existing_args: &[String],
) -> AppResult<Vec<String>> {
    if !existing_args.is_empty() {
        return Ok(existing_args.to_vec());
    }
    let mut collected = Vec::new();
    for idx in 0..param_count {
        let value = read_prompt(
            reader,
            writer,
            &format!("Enter value for argument {}: ", idx + 1),
        )?;
        collected.push(value);
    }
    Ok(collected)
}

/// Resolve the contract ID, prompting when `--id` was not supplied.
///
/// The prompted value is validated (64-hex or `C…` strkey) and re-prompted
/// on mismatch, since invoking without a valid ID cannot simulate.
fn prompt_contract_id(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    contract_id: Option<&str>,
) -> AppResult<Option<String>> {
    if let Some(id) = contract_id {
        return Ok(Some(id.to_string()));
    }
    loop {
        let input = read_prompt(reader, writer, "Enter contract ID (64-hex or C… strkey): ")?;
        match xdr_helper::parse_contract_id(&input) {
            Ok(_) => return Ok(Some(input)),
            Err(e) => {
                writeln!(writer, "Invalid contract ID: {e}. Try again.")?;
            }
        }
    }
}

/// The `key` half of a `key=value` entry, or `None` for bare values.
fn arg_key(arg: &str) -> Option<&str> {
    arg.split_once('=').map(|(key, _)| key)
}

/// Write `prompt`, flush, and read one trimmed line.
///
/// EOF (Ctrl-D, closed pipe) aborts with a clear error instead of hanging
/// or panicking.
fn read_prompt(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    prompt: &str,
) -> AppResult<String> {
    writer.write_all(prompt.as_bytes())?;
    writer.flush()?;
    let mut buf = String::new();
    let bytes = reader.read_line(&mut buf)?;
    if bytes == 0 {
        return Err(AppError::General(
            "interactive input cancelled (EOF on stdin)".to_string(),
        ));
    }
    Ok(buf.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn increment_function() -> FunctionInfo {
        FunctionInfo {
            name: "increment".to_string(),
            param_count: 1,
            result_count: 1,
            params: vec![ParamInfo {
                name: "step".to_string(),
                type_name: "i64".to_string(),
                type_def: stellar_xdr::ScSpecTypeDef::I64,
            }],
        }
    }

    fn two_param_function() -> FunctionInfo {
        FunctionInfo {
            name: "add".to_string(),
            param_count: 2,
            result_count: 1,
            params: vec![
                ParamInfo {
                    name: "a".to_string(),
                    type_name: "i64".to_string(),
                    type_def: stellar_xdr::ScSpecTypeDef::I64,
                },
                ParamInfo {
                    name: "flag".to_string(),
                    type_name: "bool".to_string(),
                    type_def: stellar_xdr::ScSpecTypeDef::Bool,
                },
            ],
        }
    }

    fn untyped_function() -> FunctionInfo {
        FunctionInfo {
            name: "raw".to_string(),
            param_count: 2,
            result_count: 1,
            params: Vec::new(),
        }
    }

    fn zero_arg_function() -> FunctionInfo {
        FunctionInfo {
            name: "ping".to_string(),
            param_count: 0,
            result_count: 1,
            params: Vec::new(),
        }
    }

    fn valid_id() -> String {
        "ab".repeat(32)
    }

    fn run(
        input: &[u8],
        functions: &[FunctionInfo],
        fn_name: Option<&str>,
        args: &[String],
        contract_id: Option<&str>,
    ) -> (AppResult<InteractiveSelection>, String) {
        let mut reader = std::io::Cursor::new(input);
        let mut writer: Vec<u8> = Vec::new();
        let result =
            prompt_for_invocation(&mut reader, &mut writer, functions, fn_name, args, contract_id);
        let output = String::from_utf8(writer).expect("prompt output is UTF-8");
        (result, output)
    }

    #[test]
    fn test_select_by_number_with_typed_arg_and_id() {
        let functions = vec![increment_function()];
        let input = format!("1\n5\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed");
        assert_eq!(selection.function, "increment");
        assert_eq!(selection.args, vec!["step=5".to_string()]);
        assert_eq!(selection.contract_id, Some(valid_id()));
        assert!(output.contains("Available functions:"));
        assert!(output.contains("Enter step (i64): "));
    }

    #[test]
    fn test_select_by_name() {
        let functions = vec![increment_function(), two_param_function()];
        let input = format!("add\n3\ntrue\n{}\n", valid_id());
        let (result, _) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed");
        assert_eq!(selection.function, "add");
        assert_eq!(selection.args, vec!["a=3".to_string(), "flag=true".to_string()]);
    }

    #[test]
    fn test_invalid_selection_reprompts() {
        let functions = vec![increment_function()];
        let input = format!("bogus\n99\n1\n5\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed after re-prompts");
        assert_eq!(selection.function, "increment");
        assert!(output.contains("Unknown selection 'bogus'"));
        assert!(output.contains("Unknown selection '99'"));
    }

    #[test]
    fn test_invalid_typed_value_reprompts() {
        let functions = vec![increment_function()];
        let input = format!("1\nabc\n5\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed after re-prompt");
        assert_eq!(selection.args, vec!["step=5".to_string()]);
        assert!(output.contains("Invalid value:"));
    }

    #[test]
    fn test_invalid_bool_value_reprompts() {
        let functions = vec![two_param_function()];
        let input = format!("1\n3\nmaybe\ntrue\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed after re-prompt");
        assert_eq!(selection.args, vec!["a=3".to_string(), "flag=true".to_string()]);
        assert!(output.contains("Invalid value:"));
    }

    #[test]
    fn test_eof_at_selection_aborts_cleanly() {
        let functions = vec![increment_function()];
        let (result, _) = run(b"", &functions, None, &[], None);
        let err = result.expect_err("EOF should abort the prompt");
        assert!(err.to_string().contains("cancelled"), "unexpected error: {err}");
    }

    #[test]
    fn test_eof_mid_prompt_aborts_cleanly() {
        let functions = vec![increment_function()];
        let (result, _) = run(b"1\n", &functions, None, &[], None);
        let err = result.expect_err("EOF mid-prompt should abort");
        assert!(err.to_string().contains("cancelled"), "unexpected error: {err}");
    }

    #[test]
    fn test_fully_specified_invocation_never_reads_stdin() {
        let functions = vec![two_param_function()];
        let args = vec!["a=3".to_string(), "flag=true".to_string()];
        // Empty stdin: any read attempt surfaces as EOF and fails the test.
        let (result, output) =
            run(b"", &functions, Some("add"), &args, Some(valid_id().as_str()));
        let selection = result.expect("no prompt should be needed");
        assert_eq!(selection.function, "add");
        assert_eq!(selection.args, args);
        assert_eq!(selection.contract_id, Some(valid_id()));
        assert!(output.is_empty(), "nothing should be printed: {output}");
    }

    #[test]
    fn test_existing_args_fill_gaps_positionally() {
        let functions = vec![two_param_function()];
        let args = vec!["flag=true".to_string()];
        let input = format!("3\n{}\n", valid_id());
        let (result, _) = run(input.as_bytes(), &functions, Some("add"), &args, None);
        let selection = result.expect("prompt should succeed");
        // Parameters stay in declaration order regardless of flag order.
        assert_eq!(selection.args, vec!["a=3".to_string(), "flag=true".to_string()]);
    }

    #[test]
    fn test_unknown_function_flag_errors() {
        let functions = vec![increment_function()];
        let (result, _) = run(b"", &functions, Some("nope"), &[], Some(valid_id().as_str()));
        let err = result.expect_err("unknown function should fail");
        assert!(err.to_string().contains("unknown function 'nope'"), "unexpected error: {err}");
    }

    #[test]
    fn test_no_functions_without_fn_errors() {
        let (result, _) = run(b"", &[], None, &[], None);
        let err = result.expect_err("empty function list should fail");
        assert!(err.to_string().contains("at least one exported function"), "unexpected error: {err}");
    }

    #[test]
    fn test_untyped_function_prompts_positionally() {
        let functions = vec![untyped_function()];
        let input = format!("hello\n42\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, Some("raw"), &[], None);
        let selection = result.expect("prompt should succeed");
        assert_eq!(selection.args, vec!["hello".to_string(), "42".to_string()]);
        assert!(output.contains("Enter value for argument 1: "));
        assert!(output.contains("Enter value for argument 2: "));
    }

    #[test]
    fn test_untyped_function_uses_existing_args_without_prompting() {
        let functions = vec![untyped_function()];
        let args = vec!["hello".to_string()];
        let (result, output) =
            run(b"", &functions, Some("raw"), &args, Some(valid_id().as_str()));
        let selection = result.expect("existing args should skip prompts");
        assert_eq!(selection.args, args);
        assert!(output.is_empty(), "nothing should be printed: {output}");
    }

    #[test]
    fn test_zero_arg_function_prompts_for_nothing() {
        let functions = vec![zero_arg_function()];
        let (result, output) =
            run(b"", &functions, Some("ping"), &[], Some(valid_id().as_str()));
        let selection = result.expect("zero-arg function needs no prompts");
        assert_eq!(selection.function, "ping");
        assert!(selection.args.is_empty());
        assert!(output.is_empty(), "nothing should be printed: {output}");
    }

    #[test]
    fn test_invalid_contract_id_reprompts() {
        let functions = vec![increment_function()];
        let input = format!("1\n5\nzzz\n{}\n", valid_id());
        let (result, output) = run(input.as_bytes(), &functions, None, &[], None);
        let selection = result.expect("prompt should succeed after re-prompt");
        assert_eq!(selection.contract_id, Some(valid_id()));
        assert!(output.contains("Invalid contract ID:"));
    }

    #[test]
    fn test_leftover_args_are_preserved_for_arity_check() {
        let functions = vec![increment_function()];
        let args = vec!["step=5".to_string(), "extra=1".to_string()];
        let (result, _) =
            run(b"", &functions, Some("increment"), &args, Some(valid_id().as_str()));
        let selection = result.expect("prompt should succeed");
        assert_eq!(selection.args, args);
    }

    #[test]
    fn test_arg_key() {
        assert_eq!(arg_key("step=5"), Some("step"));
        assert_eq!(arg_key("bare"), None);
        assert_eq!(arg_key("k=v=w"), Some("k"));
    }
}
