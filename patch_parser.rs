
impl WasmInfo {
    /// Validates WASM memory and table constraints against network limits before simulation.
    pub fn validate_wasm_limits(&self, max_size: u32, max_pages: u32) -> Result<(), crate::error::AppError> {
        let size = self.bytes.len() as u32;
        if size > max_size {
            return Err(crate::error::AppError::WasmValidation(format!(
                "WASM file size {} exceeds maximum deployable size {}",
                size, max_size
            )));
        }

        for memory in &self.memories {
            if memory.initial_pages > max_pages as u64 {
                return Err(crate::error::AppError::WasmValidation(format!(
                    "WASM memory initial pages {} exceeds limit {}",
                    memory.initial_pages, max_pages
                )));
            }
            if let Some(maximum_pages) = memory.maximum_pages {
                if maximum_pages > max_pages as u64 {
                    return Err(crate::error::AppError::WasmValidation(format!(
                        "WASM memory maximum pages {} exceeds limit {}",
                        maximum_pages, max_pages
                    )));
                }
            } else {
                return Err(crate::error::AppError::WasmValidation(format!(
                    "WASM memory maximum pages is unbounded (exceeds limit {})",
                    max_pages
                )));
            }
        }
        Ok(())
    }
}
