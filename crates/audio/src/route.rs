//! Exact selections on the ordinary control thread. Names grant no authority.
//! Candidate approval is not route continuity, Start permission or model readiness.
use crate::{notifications::ChangeSignal, realtime::OutputGate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Capture,
    Render,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin<'a> {
    Unknown,
    Physical { evidence: &'a str },
    Software { parent: &'a str, evidence: &'a str },
}
/// Observed by trusted local control code, never inferred from friendly names.
/// Software parent/driver evidence must be bound to this exact UID, not supplied
/// by UI text. This data type does not authenticate arbitrary caller claims.
#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub uid: &'a str,
    pub direction: Direction,
    pub active: bool,
    pub exact_format: bool,
    pub origin: Origin<'a>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    InvalidUid,
    Missing,
    Replaced,
    WrongDirection,
    Inactive,
    UnsupportedFormat,
    UnknownOrigin,
    PhysicalVirtualEndpoint,
    DifferentSoftwareParent,
    Feedback,
    DeviceChanged,
    Retired,
    PhysicalSourceRequired,
}
fn token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.contains('\0')
}
fn same(a: &str, b: &str) -> bool {
    // Windows MMDevice/PnP persistent IDs are compared case-insensitively.
    // No name, prefix, fuzzy match or default-device search is involved.
    a.eq_ignore_ascii_case(b)
}
fn check(observed: Observation<'_>, direction: Direction) -> Result<(), RouteError> {
    if !token(observed.uid) {
        return Err(RouteError::InvalidUid);
    }
    if observed.direction != direction {
        return Err(RouteError::WrongDirection);
    }
    if !observed.active {
        return Err(RouteError::Inactive);
    }
    if !observed.exact_format {
        return Err(RouteError::UnsupportedFormat);
    }
    Ok(())
}
/// Saved UIDs and exact parent evidence. Allocates only at control construction.
/// Revalidation failure is permanent for this selection; construct a new epoch.
pub struct RouteSelection {
    render: String,
    capture: String,
    source: String,
    parent: String,
    render_evidence: String,
    capture_evidence: String,
    source_evidence: String,
    retired: bool,
}
impl RouteSelection {
    /// Diagnostic virtual pair only. This cannot validate a production source.
    pub fn virtual_candidate(
        render: Observation<'_>,
        capture: Observation<'_>,
    ) -> Result<Self, RouteError> {
        check(render, Direction::Render)?;
        check(capture, Direction::Capture)?;
        if same(render.uid, capture.uid) {
            return Err(RouteError::Feedback);
        }
        fn software(origin: Origin<'_>) -> Result<(&str, &str), RouteError> {
            match origin {
                Origin::Software { parent, evidence } if token(parent) && token(evidence) => {
                    Ok((parent, evidence))
                }
                Origin::Physical { .. } => Err(RouteError::PhysicalVirtualEndpoint),
                _ => Err(RouteError::UnknownOrigin),
            }
        }
        let (parent, render_evidence) = software(render.origin)?;
        let (capture_parent, capture_evidence) = software(capture.origin)?;
        if !same(parent, capture_parent) {
            return Err(RouteError::DifferentSoftwareParent);
        }
        Ok(Self {
            render: render.uid.to_owned(),
            capture: capture.uid.to_owned(),
            source: String::new(),
            parent: parent.to_owned(),
            render_evidence: render_evidence.to_owned(),
            capture_evidence: capture_evidence.to_owned(),
            source_evidence: String::new(),
            retired: false,
        })
    }
    pub fn candidate(
        render: Observation<'_>,
        capture: Observation<'_>,
        source: Observation<'_>,
    ) -> Result<Self, RouteError> {
        let mut result = Self::virtual_candidate(render, capture)?;
        check(source, Direction::Capture)?;
        if same(render.uid, capture.uid)
            || same(source.uid, capture.uid)
            || same(source.uid, render.uid)
        {
            return Err(RouteError::Feedback);
        }
        let source_evidence = match source.origin {
            Origin::Physical { evidence } if token(evidence) => evidence,
            _ => return Err(RouteError::UnknownOrigin),
        };
        result.source = source.uid.to_owned();
        result.source_evidence = source_evidence.to_owned();
        Ok(result)
    }
    /// Separate diagnostic scope: no invented physical observation. A production
    /// source selection cannot use this to bypass its required source recheck.
    pub fn revalidate_pair(
        &mut self,
        render: Option<Observation<'_>>,
        capture: Option<Observation<'_>>,
        output: &OutputGate,
    ) -> Result<(), RouteError> {
        let result = (|| {
            if self.retired {
                return Err(RouteError::Retired);
            }
            if !self.source.is_empty() {
                return Err(RouteError::PhysicalSourceRequired);
            }
            let r = render.ok_or(RouteError::Missing)?;
            let c = capture.ok_or(RouteError::Missing)?;
            if !same(&self.render, r.uid) || !same(&self.capture, c.uid) {
                return Err(RouteError::Replaced);
            }
            let fresh = Self::virtual_candidate(r, c)?;
            if !same(&self.parent, &fresh.parent)
                || self.render_evidence != fresh.render_evidence
                || self.capture_evidence != fresh.capture_evidence
            {
                return Err(RouteError::Replaced);
            }
            Ok(())
        })();
        if result.is_err() {
            self.retired = true;
            output.fail();
        }
        result
    }
    pub fn is_retired(&self) -> bool {
        self.retired
    }
    /// Current snapshots of all three EXACT saved IDs, queried on control only.
    /// Missing/removal never selects an identically named replacement.
    pub fn revalidate(
        &mut self,
        render: Option<Observation<'_>>,
        capture: Option<Observation<'_>>,
        source: Option<Observation<'_>>,
        output: &OutputGate,
    ) -> Result<(), RouteError> {
        let result = (|| {
            if self.retired {
                return Err(RouteError::Retired);
            }
            if self.source.is_empty() {
                return Err(RouteError::PhysicalSourceRequired);
            }
            let (r, c, s) = (
                render.ok_or(RouteError::Missing)?,
                capture.ok_or(RouteError::Missing)?,
                source.ok_or(RouteError::Missing)?,
            );
            if !same(&self.render, r.uid)
                || !same(&self.capture, c.uid)
                || !same(&self.source, s.uid)
            {
                return Err(RouteError::Replaced);
            }
            let current = Self::candidate(r, c, s)?;
            if !same(&self.parent, &current.parent)
                || self.render_evidence != current.render_evidence
                || self.capture_evidence != current.capture_evidence
                || self.source_evidence != current.source_evidence
            {
                return Err(RouteError::Replaced);
            }
            Ok(())
        })();
        if result.is_err() {
            self.retired = true;
            output.fail();
        }
        result
    }
    /// Atomic packet-side check, before AND after operations. No COM query,
    /// allocation, waiting or cleanup. Signal batching cannot clear this fault.
    /// Register the route watch before querying snapshots; retain it through
    /// close. Native stream watches additionally guard their own exact owners.
    pub fn check_changes(
        &mut self,
        signal: &ChangeSignal,
        output: &OutputGate,
    ) -> Result<(), RouteError> {
        if self.retired || signal.has_changed() {
            self.retired = true;
            output.fail();
            Err(RouteError::DeviceChanged)
        } else {
            Ok(())
        }
    }
}
