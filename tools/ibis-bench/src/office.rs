// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Late-bound Office COM calls, confined to an instance created for a benchmark.
use anyhow::{Context, Result};
use std::path::Path;
use windows::{
    Win32::System::{
        Com::*,
        Variant::{VARIANT, VT_DISPATCH},
    },
    core::{GUID, HSTRING, PCWSTR},
};

pub struct Dispatch(pub IDispatch);

/// Do not mistake PowerPoint's stationary loading logo for document content.
/// COM may reject reads while opening; the caller polls again on the next tick.
pub fn presentation_ready(source: &Path) -> bool {
    (|| -> Result<bool> {
        use windows::{Win32::System::Ole::GetActiveObject, core::Interface};
        let name = HSTRING::from("PowerPoint.Application");
        let clsid = unsafe { CLSIDFromProgID(PCWSTR(name.as_ptr()))? };
        let mut unknown = None;
        unsafe {
            GetActiveObject(&clsid, None, &mut unknown)?;
        }
        let app = Dispatch(unknown.context("PowerPoint not registered")?.cast()?);
        let document = app.get("ActivePresentation")?;
        let path = document.invoke("FullName", DISPATCH_PROPERTYGET, vec![])?;
        anyhow::ensure!(
            path.vt() == windows::Win32::System::Variant::VT_BSTR,
            "no presentation path"
        );
        let actual = unsafe { path.Anonymous.Anonymous.Anonymous.bstrVal.to_string() };
        let expected = source.to_string_lossy().replace('/', "\\");
        let count = i32::try_from(&document.get("Slides")?.invoke(
            "Count",
            DISPATCH_PROPERTYGET,
            vec![],
        )?)?;
        Ok(actual.eq_ignore_ascii_case(&expected) && count > 0)
    })()
    .unwrap_or(false)
}

impl Dispatch {
    fn invoke(&self, name: &str, flags: DISPATCH_FLAGS, mut args: Vec<VARIANT>) -> Result<VARIANT> {
        let name = HSTRING::from(name);
        let names = [PCWSTR(name.as_ptr())];
        let mut id = 0;
        let mut named = -3; // DISPID_PROPERTYPUT
        args.reverse();
        let params = DISPPARAMS {
            rgvarg: args.as_mut_ptr(),
            cArgs: args.len() as u32,
            rgdispidNamedArgs: if flags == DISPATCH_PROPERTYPUT {
                &mut named
            } else {
                std::ptr::null_mut()
            },
            cNamedArgs: u32::from(flags == DISPATCH_PROPERTYPUT),
        };
        let mut result = VARIANT::default();
        let mut exception = EXCEPINFO::default();
        unsafe {
            self.0
                .GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0, &mut id)?;
            self.0
                .Invoke(
                    id,
                    &GUID::zeroed(),
                    0,
                    flags,
                    &params,
                    Some(&mut result),
                    Some(&mut exception),
                    None,
                )
                .with_context(|| format!("COM call {}: {}", name, *exception.bstrDescription))?;
        }
        Ok(result)
    }
    pub fn get(&self, name: &str) -> Result<Self> {
        Self::from_variant(self.invoke(name, DISPATCH_PROPERTYGET, vec![])?)
    }
    pub fn call(&self, name: &str, args: Vec<VARIANT>) -> Result<VARIANT> {
        self.invoke(name, DISPATCH_METHOD, args)
    }
    pub fn set(&self, name: &str, value: VARIANT) -> Result<()> {
        self.invoke(name, DISPATCH_PROPERTYPUT, vec![value])?;
        Ok(())
    }
    pub fn from_variant(value: VARIANT) -> Result<Self> {
        anyhow::ensure!(
            value.vt() == VT_DISPATCH,
            "Office did not return a dispatch object"
        );
        let dispatch = unsafe {
            value
                .Anonymous
                .Anonymous
                .Anonymous
                .pdispVal
                .as_ref()
                .context("null dispatch")?
                .clone()
        };
        Ok(Self(dispatch))
    }
    pub fn create(class: &str) -> Result<Self> {
        let name = HSTRING::from(class);
        let clsid = unsafe { CLSIDFromProgID(PCWSTR(name.as_ptr()))? };
        Ok(Self(unsafe {
            CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)?
        }))
    }
}

pub fn convert(app: &str, source: &Path, output: &Path) -> Result<()> {
    let class = match app {
        "msoffice-word" => "Word.Application",
        "msoffice-excel" => "Excel.Application",
        "msoffice-powerpoint" => "PowerPoint.Application",
        _ => anyhow::bail!("unknown Office app"),
    };
    let office = Dispatch::create(class)?;
    let result = (|| -> Result<()> {
        office.set(
            "DisplayAlerts",
            if app == "msoffice-powerpoint" {
                1i32.into()
            } else {
                0i32.into()
            },
        )?;
        let source: VARIANT = source.to_string_lossy().replace('/', "\\").as_str().into();
        let output: VARIANT = output.to_string_lossy().replace('/', "\\").as_str().into();
        let (collection, open_args) = match app {
            "msoffice-word" => ("Documents", vec![source, false.into(), true.into()]),
            "msoffice-excel" => ("Workbooks", vec![source, 0i32.into(), true.into()]),
            _ => (
                "Presentations",
                vec![source, true.into(), false.into(), false.into()],
            ),
        };
        let document = Dispatch::from_variant(office.get(collection)?.call("Open", open_args)?)?;
        let export = match app {
            "msoffice-word" => document.call("ExportAsFixedFormat", vec![output, 17i32.into()]),
            "msoffice-excel" => document.call("ExportAsFixedFormat", vec![0i32.into(), output]),
            _ => document.call("SaveAs", vec![output, 32i32.into()]),
        };
        let close = if app == "msoffice-powerpoint" {
            document.set("Saved", (-1i32).into())?;
            document.call("Close", vec![])
        } else {
            document.call("Close", vec![false.into()])
        };
        export?;
        close?;
        Ok(())
    })();
    let quit = office.call("Quit", vec![]);
    result?;
    quit?;
    Ok(())
}

pub fn close_fixture(app: &str, root: &Path) -> Result<()> {
    use windows::{Win32::System::Ole::GetActiveObject, core::Interface};
    let (class, collection, active) = match app {
        "msoffice-word" => ("Word.Application", "Documents", "ActiveDocument"),
        "msoffice-excel" => ("Excel.Application", "Workbooks", "ActiveWorkbook"),
        "msoffice-powerpoint" => (
            "PowerPoint.Application",
            "Presentations",
            "ActivePresentation",
        ),
        _ => anyhow::bail!("unknown Office app"),
    };
    let name = HSTRING::from(class);
    let clsid = unsafe { CLSIDFromProgID(PCWSTR(name.as_ptr()))? };
    let mut unknown = None;
    unsafe {
        GetActiveObject(&clsid, None, &mut unknown)?;
    }
    let office = Dispatch(unknown.context("Office not registered in ROT")?.cast()?);
    let count = i32::try_from(&office.get(collection)?.invoke(
        "Count",
        DISPATCH_PROPERTYGET,
        vec![],
    )?)?;
    anyhow::ensure!(
        count <= 1,
        "additional Office documents opened; left untouched"
    );
    if count == 1 {
        let document = office.get(active)?;
        let value = document.invoke("FullName", DISPATCH_PROPERTYGET, vec![])?;
        anyhow::ensure!(
            value.vt() == windows::Win32::System::Variant::VT_BSTR,
            "document path unavailable"
        );
        let full_name = unsafe { value.Anonymous.Anonymous.Anonymous.bstrVal.to_string() };
        let fixture_dir = root
            .join("target/benchmark-windows")
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        anyhow::ensure!(
            full_name
                .to_lowercase()
                .starts_with(&format!("{fixture_dir}\\")),
            "Office has a non-benchmark document; left untouched"
        );
        if app == "msoffice-powerpoint" {
            document.set("Saved", (-1i32).into())?;
            document.call("Close", vec![])?;
        } else {
            document.call("Close", vec![false.into()])?;
        }
    }
    office.call("Quit", vec![])?;
    Ok(())
}

/// Excel may not register its COM object during a first-view-only run.
/// Use the normal window close only for this specific missing-ROT error and
/// the launcher's verified owned window; save prompts are never accepted here.
pub fn close_owned_fixture(app: &str, root: &Path, hwnd: usize, pid: u32) -> Result<()> {
    anyhow::ensure!(
        crate::platform::windows()?
            .iter()
            .any(|w| w.hwnd == hwnd && w.pid == pid),
        "Office fixture ownership mismatch; left untouched"
    );
    match close_fixture(app, root) {
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<windows::core::Error>()
                    .is_some_and(|e| e.code().0 as u32 == 0x800401E3)
            }) =>
        {
            eprintln!("Office object missing from ROT; normal close of owned pid {pid}");
            crate::platform::close_fixture(hwnd, pid)
        }
        result => result,
    }
}
