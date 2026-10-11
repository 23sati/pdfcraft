//! The Windows spooler, in this process: no PowerShell, script or other program is started, so
//! printing works where scripts are locked down (App Control for Business or AppLocker run
//! PowerShell in Constrained Language Mode; issue #1107).
//!
//! The printers come from `EnumPrinters` (local queues and the user's connections, such as
//! `\\server\queue`) and the default from `GetDefaultPrinter`. A job is drawn by the in-box
//! `Windows.Data.Pdf` with Direct2D into the XPS print API, as vectors, and its options (copies,
//! collation, two-sided, colour, paper, orientation, no driver scaling) travel as a PrintTicket
//! that the driver validates before anything is sent. `winprint` and `winsafe` wrap the Win32
//! and WinRT calls, so there is no `unsafe` here.

use std::io::Write as _;

use winprint::printer::{FilePrinter as _, PrinterDevice, WinPdfPrinter};
use winprint::ticket::document::{NS_PSF, NS_PSK, OwnedName, PrintFeature, PrintTicketDocument, reader::ParsableXmlDocument as _};
use winprint::ticket::{PrintCapabilities, PrintTicket, PrintTicketBuilder};

use super::{Duplex, Job, Printer};
use crate::PrintError;

fn spool_error(e: impl std::fmt::Display) -> PrintError {
    PrintError::Spool(e.to_string())
}

/// The user's default printer; `None` when there is none.
fn default_printer() -> Option<String> {
    winsafe::GetDefaultPrinter().ok().filter(|n| !n.is_empty())
}

pub(super) fn printers() -> Vec<Printer> {
    match PrinterDevice::all() {
        Ok(devices) => super::printer_list(devices.iter().map(|d| d.name().to_string()), default_printer().as_deref()),
        Err(_) => Vec::new(),
    }
}

/// The queue the job goes to: the one asked for, else the default. Never another one: a job
/// for a queue that has gone must not come out of a different printer.
fn select_device(devices: Vec<PrinterDevice>, requested: Option<&str>, default: Option<&str>) -> Result<PrinterDevice, PrintError> {
    let name = requested.or(default).ok_or_else(|| spool_error("there is no default printer; choose a printer"))?;
    devices
        .into_iter()
        .find(|p| p.name() == name)
        .ok_or_else(|| spool_error(format!("the printer is not available: {name} (reopen Print to refresh the list)")))
}

/// The sheet's size in micrometres, short side first. `None` for a size no printer takes.
fn micrometres(width: f64, height: f64) -> Option<(u32, u32)> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 || width.max(height) > 14_400.0 {
        return None;
    }
    // At most 200 inches (14 400 pt), so at most 5 080 000 µm: the casts can't overflow.
    let um = |pt: f64| (pt * 25_400.0 / 72.0).round() as u32;
    Some((um(width.min(height)), um(width.max(height))))
}

/// The PrintTicket PdfCraft asks for: the dialog's options, the sheet's paper and orientation,
/// and no scaling (the sheets are laid out at their final size). Only schema keywords and
/// bounded numbers go into the XML; the printer name and the document title never do.
fn ticket_xml(job: &Job, width: f64, height: f64) -> Result<String, PrintError> {
    let (w, h) = micrometres(width, height).ok_or_else(|| spool_error("the sheet's size is not one a printer can take"))?;
    let orientation = if width > height { "Landscape" } else { "Portrait" };
    let duplex = match job.duplex {
        Duplex::Off => "OneSided",
        Duplex::LongEdge => "TwoSidedLongEdge",
        Duplex::ShortEdge => "TwoSidedShortEdge",
    };
    let collate = if job.collate { "Collated" } else { "Uncollated" };
    let color = if job.grayscale { "Monochrome" } else { "Color" };
    let copies = job.copies.clamp(1, 999);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<psf:PrintTicket version="1" xmlns:psf="http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework" xmlns:psk="http://schemas.microsoft.com/windows/2003/08/printing/printschemakeywords" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
 <psf:ParameterInit name="psk:JobCopiesAllDocuments"><psf:Value xsi:type="xsd:integer">{copies}</psf:Value></psf:ParameterInit>
 <psf:Feature name="psk:DocumentCollate"><psf:Option name="psk:{collate}"/></psf:Feature>
 <psf:Feature name="psk:JobDuplexAllDocumentsContiguously"><psf:Option name="psk:{duplex}"/></psf:Feature>
 <psf:Feature name="psk:PageOutputColor"><psf:Option name="psk:{color}"/></psf:Feature>
 <psf:Feature name="psk:PageScaling"><psf:Option name="psk:None"/></psf:Feature>
 <psf:Feature name="psk:PageOrientation"><psf:Option name="psk:{orientation}"/></psf:Feature>
 <psf:Feature name="psk:PageMediaSize"><psf:Option>
  <psf:ScoredProperty name="psk:MediaSizeWidth"><psf:Value xsi:type="xsd:integer">{w}</psf:Value></psf:ScoredProperty>
  <psf:ScoredProperty name="psk:MediaSizeHeight"><psf:Value xsi:type="xsd:integer">{h}</psf:Value></psf:ScoredProperty>
 </psf:Option></psf:Feature>
</psf:PrintTicket>"#
    ))
}

/// The job file's name, which the spooler shows as the document's: the title with anything
/// that could leave the folder or name a device (`CON`, `..`, `:`) replaced.
fn job_filename(title: &str) -> String {
    let title = title.strip_suffix(".pdf").or_else(|| title.strip_suffix(".PDF")).unwrap_or(title);
    let safe: String = title.chars().take(100).map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' }).collect();
    format!("PdfCraft-{safe}.pdf")
}

/// The ticket the driver accepted for this job: [`ticket_xml`] with the driver's own paper of
/// the sheet's size (its exact option, so a vendor's sizes survive; a ticket with dimensions
/// only can come back as Letter) and its own "no scaling" option, merged and validated against
/// the printer's capabilities. Refused when the driver has no such paper or changed the paper,
/// the orientation or the scaling: then nothing is sent, rather than a job at the wrong size.
fn driver_ticket(device: &PrinterDevice, job: &Job, width: f64, height: f64) -> Result<PrintTicket, PrintError> {
    let requested = PrintTicket::from_xml(ticket_xml(job, width, height)?);
    let (w, h) = micrometres(width, height).ok_or_else(|| spool_error("the sheet's size is not one a printer can take"))?;
    let capabilities = PrintCapabilities::fetch(device).map_err(spool_error)?;
    let mut candidates: Vec<_> = capabilities
        .page_media_sizes()
        .filter(|m| {
            let s = m.size();
            s.width_in_micron().abs_diff(w) <= 250 && s.height_in_micron().abs_diff(h) <= 250
        })
        .collect();
    // The closest first; on a tie the driver's own order (stable sort).
    candidates.sort_by_key(|m| m.size().width_in_micron().abs_diff(w) + m.size().height_in_micron().abs_diff(h));
    let media = candidates.into_iter().next().ok_or_else(|| {
        spool_error(format!(
            "{} has no {:.2} × {:.2} inch paper; choose a paper size it has, or add this one in its driver",
            device.name(),
            width / 72.0,
            height / 72.0
        ))
    })?;
    let mut document = PrintTicketDocument::parse_from_bytes(requested.get_xml()).map_err(spool_error)?;
    let media_ticket: PrintTicket = media.into();
    let media_document = PrintTicketDocument::parse_from_bytes(media_ticket.get_xml()).map_err(spool_error)?;
    document.features.retain(|f| !(f.name.local_name == "PageMediaSize" && f.name.namespace_ref() == Some(NS_PSK)));
    document.features.extend(media_document.features);
    document.parameter_inits.extend(media_document.parameter_inits);
    let scaling_name = OwnedName::qualified("PageScaling", NS_PSK, Some("psk"));
    if let Some(no_scaling) = capabilities.options_for_feature(scaling_name.clone()).find(|o| o.name.as_ref().is_some_and(|n| n.local_name == "None"))
    {
        document.features.retain(|f| !(f.name.local_name == "PageScaling" && f.name.namespace_ref() == Some(NS_PSK)));
        document.features.push(PrintFeature { name: scaling_name, properties: Vec::new(), options: vec![no_scaling.clone()], features: Vec::new() });
    }
    let mut builder = PrintTicketBuilder::new(device).map_err(spool_error)?;
    builder.merge(PrintTicket::from(document)).map_err(spool_error)?;
    let validated = builder.build().map_err(spool_error)?;
    let xml = std::str::from_utf8(validated.get_xml()).map_err(spool_error)?;
    verify_ticket(xml, w, h, width > height)?;
    Ok(validated)
}

/// Check the driver's ticket kept the sheet's paper and orientation and added no scaling.
/// Only those schema fields are read: some drivers return empty vendor parameters that a
/// stricter parser would refuse in an otherwise valid ticket.
fn verify_ticket(xml: &str, width: u32, height: u32, landscape: bool) -> Result<(), PrintError> {
    let document = roxmltree::Document::parse(xml).map_err(spool_error)?;
    let named = |node: roxmltree::Node<'_, '_>, local: &str| {
        node.attribute("name")
            .and_then(|v| v.split_once(':'))
            .is_some_and(|(prefix, name)| name == local && node.lookup_namespace_uri(Some(prefix)) == Some(NS_PSK))
    };
    let feature = |name: &str| document.root_element().children().find(|n| n.has_tag_name((NS_PSF, "Feature")) && named(*n, name));
    let media = feature("PageMediaSize").ok_or_else(|| spool_error("the printer's driver did not confirm the paper size; nothing was sent"))?;
    let dimension = |name: &str| {
        media
            .descendants()
            .find(|n| n.has_tag_name((NS_PSF, "ScoredProperty")) && named(*n, name))
            .and_then(|n| n.children().find(|v| v.has_tag_name((NS_PSF, "Value"))))
            .and_then(|n| n.text())
            .and_then(|v| v.trim().parse::<u32>().ok())
    };
    if dimension("MediaSizeWidth").is_none_or(|w| w.abs_diff(width) > 250) || dimension("MediaSizeHeight").is_none_or(|h| h.abs_diff(height) > 250) {
        return Err(spool_error("the printer's driver changed the paper size; nothing was sent (choose a paper size it has)"));
    }
    let orientation = feature("PageOrientation").and_then(|f| f.children().find(|n| n.has_tag_name((NS_PSF, "Option"))));
    if !orientation.is_some_and(|n| named(n, if landscape { "Landscape" } else { "Portrait" })) {
        return Err(spool_error("the printer's driver changed the orientation; nothing was sent"));
    }
    if let Some(scaling) = feature("PageScaling") {
        // Microsoft's IPP class driver names its "None" in a namespace of its own.
        if !scaling.children().any(|n| n.has_tag_name((NS_PSF, "Option")) && n.attribute("name").and_then(|v| v.rsplit(':').next()) == Some("None")) {
            return Err(spool_error("the printer's driver would scale the sheets; nothing was sent"));
        }
    }
    Ok(())
}

/// Spool a print-ready PDF. Windows.Data.Pdf reads from a path, so the job goes to a file in a
/// private temporary folder (an unpredictable name, only this user's) for as long as the spooler
/// takes it, and the folder is removed however the job ended. Windows answers once the job is in
/// the queue: a paused queue or an offline printer keeps the job there, as with any program.
pub(super) fn submit(pdf: &[u8], job: &Job) -> Result<String, PrintError> {
    let device = select_device(PrinterDevice::all().map_err(spool_error)?, job.printer.as_deref(), default_printer().as_deref())?;
    let super::OneShape { pdf, sheet, count } = super::one_sheet_shape(pdf)?;
    let dir =
        tempfile::Builder::new().prefix("pdfcraft-print-").tempdir().map_err(|e| spool_error(format!("the print job could not be written: {e}")))?;
    let path = dir.path().join(job_filename(&job.title));
    let written = |e: std::io::Error| spool_error(format!("the print job could not be written: {e}"));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(written)?;
    file.write_all(&pdf).map_err(written)?;
    drop(file);
    let job = job.clone();
    // Its own thread, so WinRT and COM get an apartment of their own whoever calls (the
    // desktop app's worker, the CLI). Joined before `dir` is dropped: the spooler reads the file.
    let result = std::thread::Builder::new()
        .name("print job".into())
        .spawn(move || {
            let ticket = driver_ticket(&device, &job, sheet.0, sheet.1)?;
            WinPdfPrinter::new(device).print(&path, ticket).map_err(|e| {
                let mut message = e.to_string();
                let mut cause = std::error::Error::source(&e);
                while let Some(source) = cause {
                    message.push_str(&format!(": {source}"));
                    cause = source.source();
                }
                spool_error(message)
            })
        })
        .map_err(|e| spool_error(format!("the print job could not be started: {e}")))?
        .join()
        .map_err(|_| spool_error("the print job stopped unexpectedly; check the printer's queue before printing again"))?;
    result?;
    Ok(format!("spooled {count} sheet(s)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticket_carries_the_job_options_and_the_sheet() {
        let job = Job { copies: 3, collate: false, duplex: Duplex::ShortEdge, grayscale: true, title: "<&>".into(), ..Job::default() };
        let xml = ticket_xml(&job, 1224.0, 792.0).unwrap();
        for value in [">3<", "psk:Uncollated", "psk:TwoSidedShortEdge", "psk:Monochrome", "psk:Landscape", ">279400<", ">431800<", "psk:None"] {
            assert!(xml.contains(value), "missing {value}");
        }
        assert!(!xml.contains("<&>"), "the title never enters the XML");
        let xml = ticket_xml(&Job { copies: 5000, ..Job::default() }, 612.0, 792.0).unwrap();
        for value in [">999<", "psk:Collated", "psk:OneSided", "psk:Color", "psk:Portrait", ">215900<"] {
            assert!(xml.contains(value), "missing {value}");
        }
        assert!(ticket_xml(&Job { duplex: Duplex::LongEdge, ..Job::default() }, 612.0, 792.0).unwrap().contains("psk:TwoSidedLongEdge"));
        assert!(ticket_xml(&job, f64::NAN, 100.0).is_err());
        assert!(ticket_xml(&job, -1.0, 100.0).is_err());
        assert!(ticket_xml(&job, 100.0, 1e9).is_err());
        assert!(roxmltree::Document::parse(&ticket_xml(&job, 612.0, 792.0).unwrap()).is_ok());
    }

    #[test]
    fn a_missing_printer_never_falls_back_to_another_queue() {
        assert!(select_device(Vec::new(), Some("missing"), Some("default")).is_err());
        assert!(select_device(Vec::new(), None, None).is_err());
    }

    #[test]
    fn the_job_file_stays_in_its_folder() {
        let name = job_filename(r"..\..\CON:/unsafe?name");
        assert_eq!(std::path::Path::new(&name).components().count(), 1);
        assert!(name.starts_with("PdfCraft-") && name.ends_with(".pdf"));
        assert_eq!(job_filename("Invoice 42.pdf"), "PdfCraft-Invoice 42.pdf");
    }

    #[test]
    fn a_driver_that_changes_the_sheet_is_refused() {
        let letter = ticket_xml(&Job::default(), 612.0, 792.0).unwrap();
        verify_ticket(&letter, 215_900, 279_400, false).unwrap();
        assert!(verify_ticket(&letter, 210_000, 297_000, false).is_err(), "A4 asked, Letter returned");
        assert!(verify_ticket(&letter, 215_900, 279_400, true).is_err(), "landscape asked, portrait returned");
        let scaled = letter.replace(
            r#"<psf:Feature name="psk:PageScaling"><psf:Option name="psk:None"/>"#,
            r#"<psf:Feature name="psk:PageScaling"><psf:Option name="psk:FitApplicationBleedSizeToPageImageableSize"/>"#,
        );
        assert!(verify_ticket(&scaled, 215_900, 279_400, false).is_err(), "scaling added");
        assert!(verify_ticket("<not xml", 215_900, 279_400, false).is_err());
    }

    /// What a real driver makes of a job's ticket: Letter, portrait, two copies, two-sided on
    /// the long edge, grayscale. Sends no job. `PDFCRAFT_TEST_PRINTER` names the queue;
    /// `PDFCRAFT_TEST_TICKET_OUTPUT` keeps the validated ticket, to see which options it kept.
    #[test]
    #[ignore = "needs PDFCRAFT_TEST_PRINTER naming an installed printer; sends no job"]
    fn an_installed_driver_validates_the_ticket() {
        let name = std::env::var("PDFCRAFT_TEST_PRINTER").unwrap();
        let device = select_device(PrinterDevice::all().unwrap(), Some(&name), None).unwrap();
        let job = Job { copies: 2, duplex: Duplex::LongEdge, grayscale: true, ..Job::default() };
        let ticket = driver_ticket(&device, &job, 612.0, 792.0).unwrap();
        if let Ok(path) = std::env::var("PDFCRAFT_TEST_TICKET_OUTPUT") {
            std::fs::write(path, ticket.get_xml()).unwrap();
        }
    }
}
