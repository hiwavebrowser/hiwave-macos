//! LAY1: a flex item with its own `aspect-ratio` is sized by the ratio, not
//! by its child.
//!
//! Found on a recorded search page (E54 review): each result card is
//! `div.su-image { display: flex; height: 100%; width: 100%;
//! align-items: flex-start }` holding `a { width: inherit; height: inherit;
//! aspect-ratio: 1 }` holding the picture. Chrome lays the link out 294 x
//! 294 and the title below it; the engine sized the link from the picture
//! (30 tall), the container collapsed to it, and the title was painted over
//! the picture.
//!
//! The expectations are Chrome for Testing 148.0.7778.216 (headless) on the
//! same pages, generated here as they were for the oracle: a 294px wide
//! wrapper, the flex container `#c` (`height: 100%`, so its height is
//! indefinite), the item `#it` with `aspect-ratio: 1`, and a paragraph
//! `#txt` after the container. Each case varies the direction, whether the
//! item is a box holding a 40 x 30 image or the image itself, its width and
//! height (`100%`, `inherit` from the container's `100%`, `auto`), its
//! `min-height` (`auto`, `0`) and `align-items`. Each rect is
//! `x,y,width,height` of the border box, as `getBoundingClientRect` reads
//! it, rounded to a tenth.

use super::*;
use crate::script_net_tests::serve_routes;

const IMG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACgAAAAeCAIAAADRv8uKAAAAKElEQVR4nO3NMQEAAAwCINPaP4IxtgMKkB6JWCwWi8VisVgsFov/xgPgw5qsBw4nKgAAAABJRU5ErkJggg==";

/// `name c=… it=… im=… txt=…`, one line per case (tools/parity_oracle
/// measure output; `im` is `-` when the item is the image).
const CHROME: &str = "\
column-box-w100p-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-w100p-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-h100p-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-h100p-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-hauto-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-hauto-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-hinherit-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-wauto-hinherit-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=0,5,40,30 txt=0,40,294,18\n\
column-box-wauto-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-box-winherit-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
column-img-w100p-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-w100p-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-h100p-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-h100p-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-hauto-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-hauto-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-hinherit-mh0-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-wauto-hinherit-mhauto-flex-start c=0,0,294,40 it=0,0,40,40 im=- txt=0,40,294,18\n\
column-img-wauto-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
column-img-winherit-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-box-w100p-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-w100p-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-wauto-h100p-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-h100p-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-h100p-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-h100p-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hauto-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hauto-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hauto-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hauto-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hinherit-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hinherit-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hinherit-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-wauto-hinherit-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=127,5,40,30 txt=0,40,294,18\n\
row-box-winherit-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-box-winherit-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
row-img-w100p-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-w100p-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-wauto-h100p-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-h100p-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-h100p-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-h100p-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hauto-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hauto-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hauto-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hauto-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hinherit-mh0-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hinherit-mh0-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hinherit-mhauto-flex-start c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-wauto-hinherit-mhauto-stretch c=0,0,294,40 it=127,0,40,40 im=- txt=0,40,294,18\n\
row-img-winherit-h100p-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-h100p-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-h100p-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-h100p-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hauto-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hauto-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hauto-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hauto-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hinherit-mh0-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hinherit-mh0-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hinherit-mhauto-flex-start c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
row-img-winherit-hinherit-mhauto-stretch c=0,0,294,294 it=0,0,294,294 im=- txt=0,294,294,18\n\
";

/// Cases outside the grid: content taller than the ratio under
/// `min-height: auto` and `0`, `inherit` of a percentage and of `em`, and
/// the card as the page writes it.
const CHROME_EXTRA: &str = "\
ebay-card c=0,0,294,294 it=0,0,294,294 im=127,132,40,30 txt=0,294,294,18\n\
inherit-block-def c=0,0,150,50 it=0,0,75,12.5 im=- txt=0,50,300,18\n\
inherit-block-pct c=0,0,147,0 it=0,0,73.5,0 im=- txt=0,0,294,18\n\
inherit-em c=0,0,200,0 it=0,0,200,0 im=- txt=0,0,800,13\n\
tall-column-mh0 c=0,0,294,50 it=0,0,50,50 im=0,0,50,100 txt=0,50,294,18\n\
tall-column-mhauto c=0,0,294,100 it=0,0,50,100 im=0,0,50,100 txt=0,100,294,18\n\
tall-row-mh0 c=0,0,294,50 it=0,0,50,50 im=0,0,50,100 txt=0,50,294,18\n\
tall-row-mhauto c=0,0,294,100 it=0,0,50,100 im=0,0,50,100 txt=0,100,294,18\n\
";

fn variant_page(name: &str) -> String {
    let parts: Vec<&str> = name.splitn(6, '-').collect();
    let [direction, item, w, h, mh, align] = parts[..] else { panic!("{name}") };
    let size = |s: &str| s[1..].replace('p', "%");
    let (w, h, mh) = (size(w), size(h), &mh[2..]);
    let item_style = format!("width:{w};height:{h};min-height:{mh};aspect-ratio:1");
    let inner = match item {
        "box" => format!(
            "<a id=it style=\"display:flex;align-items:center;justify-content:center;{item_style}\">\
             <img id=im src=\"{IMG}\" style=\"max-height:100%;max-width:100%\"></a>"
        ),
        _ => format!("<img id=it src=\"{IMG}\" style=\"{item_style}\">"),
    };
    format!(
        "<!doctype html><html><head><style>body{{margin:0}}p{{margin:0}}</style></head><body><div style=\"width:294px\">\
         <div id=c style=\"display:flex;flex-direction:{direction};height:100%;width:100%;justify-content:center;align-items:{align}\">\
         {inner}</div><p id=txt>text</p></div></body></html>"
    )
}

fn extra_page(name: &str) -> String {
    let head = "<!doctype html><html><head><style>body{margin:0}p{margin:0}</style></head><body><div style=\"width:294px\">";
    let tail = "<p id=txt>text</p></div></body></html>";
    match name {
        "ebay-card" => format!(
            "{head}<div class=su-image id=c style=\"position:relative;display:flex;height:100%;width:100%;justify-content:center;align-items:flex-start\">\
             <a id=it style=\"width:inherit;height:inherit;aspect-ratio:1;display:flex;align-items:center;justify-content:center;border-radius:16px;overflow:hidden\">\
             <img id=im src=\"{IMG}\" style=\"max-height:100%;max-width:100%\"></a></div>{tail}"
        ),
        "inherit-block-pct" => format!("{head}<div id=c style=\"width:50%;height:25%\"><div id=it style=\"width:inherit;height:inherit\"></div></div>{tail}"),
        "inherit-block-def" => "<!doctype html><html><head><style>body{margin:0}p{margin:0}</style></head><body><div style=\"width:300px;height:200px\">\
            <div id=c style=\"width:50%;height:25%\"><div id=it style=\"width:inherit;height:inherit\"></div></div><p id=txt>t</p></div></body></html>"
            .to_string(),
        "inherit-em" => "<!doctype html><html><head><style>body{margin:0;font-size:10px}p{margin:0}</style></head><body>\
            <div id=c style=\"width:10em;font-size:20px\"><div id=it style=\"width:inherit;font-size:5px\"></div></div><p id=txt>t</p></body></html>"
            .to_string(),
        tall => {
            let (_, rest) = tall.split_once('-').unwrap();
            let (direction, mh) = rest.split_once("-mh").unwrap();
            format!(
                "{head}<div id=c style=\"display:flex;flex-direction:{direction};align-items:flex-start\">\
                 <div id=it style=\"width:50px;aspect-ratio:1;min-height:{mh}\"><div id=im style=\"height:100px\"></div></div></div>{tail}"
            )
        }
    }
}

/// What the engine reads, in the oracle's format.
fn measure(page: &str) -> String {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let value = engine
        .execute_script(
            view,
            "String((function(){var q=function(s){var e=document.querySelector(s);if(!e)return '-';var b=e.getBoundingClientRect();\
             return [b.x,b.y,b.width,b.height].map(function(v){return Math.round(v*10)/10}).join(',')};\
             return 'c='+q('#c')+' it='+q('#it')+' im='+q('#im')+' txt='+q('#txt')})())",
        )
        .unwrap();
    value.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&value).to_string()
}

fn mismatches(table: &str, page: fn(&str) -> String) -> Vec<String> {
    table
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let (name, chrome) = line.split_once(' ').unwrap();
            let ours = measure(&page(name));
            (ours != chrome).then(|| format!("{name}\n  chrome {chrome}\n  engine {ours}"))
        })
        .collect()
}

/// The 144 cases of the grid, each as Chrome lays it out.
#[test]
fn a_flex_item_with_an_aspect_ratio_is_sized_as_in_chrome() {
    let wrong = mismatches(CHROME, variant_page);
    assert!(wrong.is_empty(), "{} of 144 differ from Chrome:\n{}", wrong.len(), wrong.join("\n"));
}

/// The automatic minimum size, `inherit`, and the card as the page has it.
#[test]
fn aspect_ratio_minimum_size_inherit_and_the_card_are_as_in_chrome() {
    let wrong = mismatches(CHROME_EXTRA, extra_page);
    assert!(wrong.is_empty(), "{} differ from Chrome:\n{}", wrong.len(), wrong.join("\n"));
}
