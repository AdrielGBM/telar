//! A canvas as the inspection panels read it: the preview it shows, without what the workshop and the preview host put around it.

use telar::preview::host::preview_nodes;
use telar::{AccessAudit, SegmentNodeInfo, Transform};
use telar_devtools::InspectSource;

/// `S`, with its walk starting at the preview's own root, so the components listed and the one chosen first are the preview's.
pub(crate) struct PreviewOnly<S>(pub(crate) S);

impl<S: InspectSource> InspectSource for PreviewOnly<S> {
    fn generation(&self) -> u64 {
        self.0.generation()
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        preview_nodes(&self.0.walk())
    }

    fn audit(&self) -> Option<AccessAudit> {
        self.0.audit()
    }

    fn placement(&self) -> Transform {
        self.0.placement()
    }
}
