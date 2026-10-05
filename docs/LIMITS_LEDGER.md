# Limits ledger

Every **stated limit** in the bodies of the pull requests merged into `develop`
between 2026-10-02 23:00 UTC and develop `15d2c3a6013e1e70593fa30656670c203a627fd2`
(#447 to #527, 78 merged PRs). A row is here only because the body of the PR it
cites states it, in a "Stated limits", "LIMITS", "Not here", "Not measured",
"Follow-ups" or similar section, or in a sentence such as "does not", "not
implemented" or "not yet". Nothing here was inferred from the code.

- **Rows:** 289, from 64 PRs. The other 14 PRs state no limit: #449, #457, #460,
  #461, #464, #469, #476, #479, #485, #491, #495, #496, #497, #524. (#496 and #497
  report measured board results, which are not limits.)
- **Retired since:** 42 rows say "Retired by #N" in the last-but-one column. That
  means a *later* PR in the same window says it does the thing. Some more say
  "Partly retired". Those rows stay, so that the history of each limit can be
  traced. Where the 2026-10-05 web API census
  ([docs/census/WEB_API_CENSUS_2026-10-05.md](census/WEB_API_CENSUS_2026-10-05.md))
  measured the outcome, the note says so.
- **What would retire it:** this is the follow-up the PR names, where it names
  one. Otherwise it is a one-line description of the missing capability.
- **Page can observe:** `yes` means a page's script or its rendering can tell the
  difference from Chrome. `no` means it affects tooling, logs or the app shell
  only. `unknown` means the body does not say enough to decide.
- **Areas:** the requested groups, plus three more for rows that fit none of
  them: *rendering and images*, *DOM and parsing* and *view host and app shell*.
  Rows are ordered by area, then by PR.
- Several PRs restate the same limit (for example the click target and the
  focus-at-press rows). Each statement is kept with its own PR.

| Area | Limit | PR | What would retire it | Page can observe |
|---|---|---:|---|---|
| layout and geometry | A float after a block collapses its top margin with the block's bottom margin, unlike Chrome. | #474 | Stop margin collapsing between a float and a preceding block. | yes |
| layout and geometry | The wordpress.org hero background box is laid out at the wrong size compared with Chrome. | #474 | Fix the layout of that hero box. | yes |
| layout and geometry | Script geometry reads come from the last layout, so a script that mutates then measures sees stale numbers. | #494 | Partly retired by #505 (layout between lifecycle steps); same-task reads still stale (#505). | yes |
| layout and geometry | Transforms are not applied to client rects. | #494 | Apply transforms to getBoundingClientRect results. | yes |
| layout and geometry | offsetParent ignores table, td and th as offset parents. | #494 | Treat table/td/th as offset parents. | yes |
| layout and geometry | Elements have no geometry in script: no getBoundingClientRect or offsetWidth family. | #498 | Retired by #494 (layout geometry for script). | yes |
| layout and geometry | A geometry read in the same task as a write still sees the layout from before the task. | #505 | Synchronous layout inside a task (option H in #476). | yes |
| layout and geometry | A closed details with no summary child renders nothing instead of a default Details summary. | #508 | Render a default Details summary | yes |
| layout and geometry | An open details whose summary is not its first child keeps DOM order instead of moving the summary to the top. | #508 | Move the summary to the top of an open details | yes |
| scrolling | scrollTop/scrollLeft setters and scrollIntoView/scrollTo are not provided. | #494 | Retired by #506 (window and element scroll for script). | yes |
| scrolling | Element scroll offsets are stored and clamped but not rendered; the engine has no per-element scrolling. | #506 | Per-element scrolling in the engine. | yes |
| scrolling | scrollIntoView scrolls the window only, not an enclosing scroll container. | #506 | Per-element scroll containers. | yes |
| scrolling | There is no smooth scrolling and no scroll-behavior, scroll-margin or scroll-snap; the behavior option is ignored. | #506 | Smooth scrolling and scroll-behavior/margin/snap. | yes |
| scrolling | User scrolling does not fire the scroll event. | #506 | Retired by #507 (scroll event on user scroll). | yes |
| scrolling | A measurement right after a script scroll in the same task reads pre-scroll geometry if layout changed. | #506 | Layout on a dirty read. | yes |
| scrolling | A fragment scroll fires no scroll event and does not update window.scrollY. | #509 | Retired by #512 (a fragment jump updates script's scroll state and fires scroll). | yes |
| scrolling | Engine::set_scroll_offset still does not publish the offset to script. | #512 | Publish set_scroll_offset to script | no |
| observers | MutationObserver and IntersectionObserver callbacks never fire. | #482 | IntersectionObserver retired by #507; MutationObserver callbacks still never fire in the 2026-10-05 census. | yes |
| observers | IntersectionObserver does not clip by ancestors' overflow, so a target in a scrolled-out overflow:hidden container reads as intersecting. | #507 | Apply ancestor overflow clips in intersection computation. | yes |
| observers | ResizeObserver has no per-frame loop-limit error. | #507 | Per-frame delivery with the loop-limit error. | yes |
| observers | Observer entries come from the layout as published, so same-task staleness applies. | #507 | Fresh layout on read before computing entries. | yes |
| Shadow DOM | Shadow DOM (attachShadow, slots, scoped style) is not provided with custom elements. | #465 | Script-visible half retired by #510; rendering and style scoping are still open (see #510 rows). | yes |
| Shadow DOM | Nothing in a shadow tree is laid out or painted. | #510 | Slice 2 (the flat tree). | yes |
| Shadow DOM | Shadow tree styles apply to nothing; scoping, :host, ::slotted and ::part are not implemented. | #510 | Slice 3 (style scoping). | yes |
| Shadow DOM | slotchange does not fire and slot.assign() is a no-op for manual slot assignment. | #510 | Implement slotchange and manual slot assignment. | yes |
| Shadow DOM | composedPath() does not hide the inside of a closed root from listeners outside it. | #510 | Hide closed-root internals in composedPath. | yes |
| Shadow DOM | Selectors in ShadowRoot.querySelector cannot see out of the root; :host and ancestor context stop at the fragment. | #510 | Selector matching across the shadow boundary. | yes |
| Shadow DOM | ShadowRoot adoptedStyleSheets are stored but apply to nothing, and activeElement is always null. | #510 | Apply shadow adoptedStyleSheets (slice 3). | yes |
| CSSOM | The :defined pseudo-class is not provided. | #465 | Implement :defined in the selector engine. | yes |
| CSSOM | The engine does not apply adoptedStyleSheets or constructed sheets. | #492 | Feed constructed and adopted sheets into the engine cascade. | yes |
| CSSOM | A sheet's disabled flag is recorded but not honoured by the engine. | #492 | Honour sheet disabled in the cascade. | yes |
| CSSOM | A link element's sheet has no rules because the bindings never see the fetched text. | #492 | Pass fetched stylesheet text to the bindings. | yes |
| CSSOM | At-rules are generic CSSRule objects with no subclasses, no nested cssRules and no insertRule into them. | #492 | CSSMediaRule and other grouping-rule subclasses. | yes |
| CSSOM | Editing any rule rewrites the style element's whole text and comments between rules are lost. | #492 | Preserve comments on rule rewrites. | yes |
| CSSOM | CSS.supports checks values only for display and position; any balanced value is accepted for other properties. | #492 | Per-property value validation in CSS.supports. | yes |
| CSSOM | sheet.media is a plain object, not a live MediaList. | #492 | A live MediaList. | yes |
| CSSOM | link rel=stylesheet reflection, getComputedStyle, matchMedia evaluation and DOMMatrix remain missing. | #492 | Partly retired: getComputedStyle by #501, DOMMatrix by #513. Still open in the 2026-10-05 census: `link` `.sheet`, `matchMedia` width evaluation. | yes |
| CSSOM | getComputedStyle is not provided by this slice. | #494 | Retired by #501 (getComputedStyle reads the cascade). | yes |
| CSSOM | getComputedStyle values come from the last layout plus inline overrides, so a class-toggled rule change shows only after the next layout. | #501 | Slice 1b (layout on a dirty read). | yes |
| CSSOM | Computed display uses the engine's own 8 values, so li and table read block. | #501 | Model list-item and table display values. | yes |
| CSSOM | Computed z-index reads auto for 0. | #501 | Distinguish z-index 0 from auto in the computed style. | yes |
| CSSOM | Properties the engine does not model, custom properties and pseudo-element styles are not provided by getComputedStyle. | #501 | Publish custom properties and pseudo-element styles. | yes |
| CSSOM | A descendant of a display:none subtree reads display: none instead of its own value. | #501 | Compute style for elements without boxes. | yes |
| CSSOM | The :target pseudo-class, scroll-margin and scroll-behavior are not supported for fragment navigation. | #509 | Implement :target, scroll-margin and scroll-behavior. | yes |
| CSSOM | DOMMatrix strings do not support perspective() or rotate3d(). | #513 | Parse perspective() and rotate3d(). | yes |
| Intl | None of the 10 probed Intl APIs work. | #482 | Retired by #493 (Intl en-US baseline); census 2026-10-05 shows 8 of the 10 rows work. | yes |
| Intl | Intl resolves every locale to en-US and formats only en-US output. | #493 | Real locale data (ICU or equivalent) for other locales. | yes |
| Intl | Only UTC and the default time zone are supported; other IANA zones format in the default zone. | #493 | IANA time zone data support. | yes |
| Intl | The default zone's resolvedOptions().timeZone is not the host's IANA name. | #493 | Report the host's IANA time zone name. | yes |
| Intl | Intl.Collator compares by code point after folding, not by the full UCA. | #493 | A full UCA collation implementation. | yes |
| Intl | Intl.Segmenter, Intl.Locale, formatRange, String.prototype.localeCompare and non-Gregorian calendars are not covered. | #493 | Implement those Intl APIs and calendars. | yes |
| forms | ElementInternals and form-associated custom elements are not provided. | #465 | Implement ElementInternals and form-associated custom elements. | yes |
| forms | Script-set selectedness marks every option in the select dirty, so later selected-attribute changes no longer move the selection. | #489 | Mark only the option that was set as dirty, per spec. | yes |
| forms | The form owner is the nearest ancestor form; the form="id" content attribute is ignored. | #489 | Honour the form content attribute. | yes |
| forms | form.elements, options and selectedOptions are static snapshots, not live collections. | #489 | Live HTMLCollections. | yes |
| forms | The submit event is a plain Event with a submitter property; there is no SubmitEvent interface. | #489 | Retired by #518 (SubmitEvent). | yes |
| forms | requestSubmit fires the submit event but does no navigation or network. | #489 | Form submission navigation. | yes |
| forms | input.files is always empty because there is no file picker. | #489 | A file picker that fills input.files. | yes |
| forms | select has no required validity in constraint validation. | #489 | Add select required validity. | yes |
| forms | The user's picks are not synced from the engine into checkedness or selectedness; only text values are synced. | #489 | Sync user checkbox/radio/select picks into the bindings. | yes |
| forms | The engine's own focus from a click does not update document.activeElement; focus is script-side only. | #490 | Retired by #517. | yes |
| forms | Clicking a checkbox or a label does not check or activate the control and fires no change event. | #498 | Retired by #502 (a click checks a checkbox and clicks a label's control). | yes |
| forms | Clicking a submit button does not submit the form. | #498 | Retired by #504 (a submit button submits its form). | yes |
| forms | Clicking a checkbox does not check it. | #500 | Retired by #502. | yes |
| forms | A label with for does nothing to its control. | #500 | Retired by #502. | yes |
| forms | A submit button does not submit its form. | #500 | Retired by #504. | yes |
| forms | A submit button does not submit its form on click. | #502 | Retired by #504. | yes |
| forms | el.matches(':checked') and querySelector(':checked') from script still read the attribute, not the live state. | #502 | Retired by #520 (:checked reads live checkedness). | yes |
| forms | A label click does not move focus to its control. | #502 | Retired by #519 (a click on a label focuses its control). | yes |
| forms | Space on a focused checkbox, indeterminate, and :checked on option from a live selection are not handled. | #502 | Implement keyboard activation, indeterminate and live option :checked | yes |
| forms | Only GET forms submit; a method=post form submits nothing. | #504 | A loader that takes a request, not just a URL. | yes |
| forms | A form with no named successful control and an unnamed button produces no navigation. | #504 | Submit forms with empty data. | yes |
| forms | target=_blank forms do not navigate. | #504 | Support form targets that open new browsing contexts. | yes |
| forms | formaction, formmethod, formtarget, the form= attribute and input type=image click coordinates are not supported. | #504 | Implement the submitter override attributes, form= association and image coordinates. | yes |
| forms | form.submit() from script is a no-op and a requestSubmit() from a timer does not navigate. | #504 | Navigate for script-initiated submissions. | yes |
| forms | A checkbox or radio with no value attribute submits an empty value instead of on. | #504 | Default the value to on. | yes |
| forms | details name groups (exclusive accordions) are not implemented. | #508 | Implement details name groups | yes |
| forms | The :checked selector in matches/querySelector, POST forms and label focus are not done. | #509 | :checked retired by #520 and label focus by #519; POST forms still open (#511). | yes |
| forms | Implicit submission ignores the rule that several fields with no submit button block implicit submission. | #511 | Implement the blocking-field rule for implicit submission. | yes |
| forms | POST forms still submit nothing. | #511 | POST form submission support. | yes |
| forms | change fires only for typing into a text field; checkbox, radio and select changes are unchanged. | #517 | Unify change firing for all controls | yes |
| forms | A label's for is looked up by id whatever the element is, so a for naming a non-field focuses nothing. | #519 | Label resolution limited to labelable elements per spec. | yes |
| forms | Only input, textarea and select count as labelled fields; a label for a button, meter, output or progress focuses nothing. | #519 | Treat all labelable elements as label targets. | yes |
| forms | option:checked still reads the selected attribute, not the select's live choice. | #520 | Live selectedness in selector matching. | yes |
| forms | Form-associated custom elements with disabled are not treated as disabled form controls. | #521 | Support form-associated custom elements. | yes |
| forms | A click retargeted to an ancestor does not focus a label's control, even when the ancestor is inside the label. | #526 | Run label activation for retargeted clicks inside a label | yes |
| history and navigation | Plain link clicks failing and the inverse resize scale are not investigated. | #480 | I0 (b) addressed by #487. | yes |
| history and navigation | history.pushState and replaceState are empty functions, so history.state stays null after the call. | #482 | Retired by #488 (history API); census 2026-10-05 shows pushState/replaceState work. | yes |
| history and navigation | History is per document: an engine navigation resets it to one entry and engine back/forward is not wired to these entries. | #488 | Cross-document session history wired to engine back/forward | yes |
| history and navigation | history.length does not count entries across documents. | #488 | Cross-document session history | yes |
| history and navigation | pushState/replaceState ignore title, scroll is not restored, and go(0) does nothing because there is no reload. | #488 | Implement scrollRestoration and reload for go(0) | yes |
| history and navigation | Several traversals queued in one turn each apply separately instead of being merged into one traversal as the spec does. | #488 | Merge queued traversals per spec | yes |
| history and navigation | Assigning location.href or location.hash does not add a history entry and does not fire hashchange. | #488 | Make location a live object that navigates and updates history | yes |
| history and navigation | Anchor href resolution ignores <base href>. | #488 | Resolve anchor URLs against the document base URL | yes |
| history and navigation | The Navigation API (window.navigation) is not provided. | #488 | Implement the Navigation API | yes |
| history and navigation | History traversal performs no real navigation or network. | #488 | Wire history traversal to real navigation | yes |
| history and navigation | Nothing in the engine scrolls to a fragment on an href="#frag" click. | #498 | Retired by #509 (a fragment link scrolls this document). | yes |
| history and navigation | href="javascript:..." links do nothing. | #498 | Retired by #509 (a javascript: link runs its script). | yes |
| history and navigation | Fragment links and javascript: links are not handled by the click fix. | #500 | Retired by #509. | yes |
| history and navigation | The app's URL bar does not show the new fragment after a fragment link. | #509 | A ClickOutcome field for a URL change without a load. | no |
| history and navigation | Only id is looked up as a fragment target; a name anchor is not. | #509 | Look up a name anchors as fragment targets. | yes |
| history and navigation | Loading a URL that already has a fragment still shows the top of the page. | #509 | Scroll to the fragment on initial load. | yes |
| history and navigation | A javascript: URL whose script returns a string does not replace the document; the result is dropped. | #509 | Replace the document with the string result of a javascript: URL. | yes |
| history and navigation | Setting location.hash or a fragment location.href from script does not scroll. | #509 | Scroll on script-initiated fragment navigation. | yes |
| history and navigation | Fragment navigation does not update the URL bar, handle <a name>, :target, a load of a URL with a fragment, or location.hash assignment. | #512 | The remaining #509 fragment-navigation items | yes |
| events and input | Inline onclick attribute handlers were not checked for live clicks. | #480 | Retired by #498 (onclick attributes run as the element's handler). | yes |
| events and input | Focus moves on mouse release, not on press. | #480 | Retired by #525 (the focus moves at the press). | yes |
| events and input | Modifier keys are always false in mouse events. | #480 | Populate modifier key state in mouse events | yes |
| events and input | No dblclick, contextmenu, mousemove or hover events are dispatched. | #480 | Move and hover retired by #523; dblclick and contextmenu still open (#521, #526). | yes |
| events and input | initEvent/initCustomEvent treat being dispatched as eventPhase !== 0. | #481 | Track a real dispatch flag on events | yes |
| events and input | Plain link clicks failing and the inverse resize scale (I0 b) are not fixed. | #486 | I0 (b) addressed by #487 (resize and move the content NSView on set_bounds). | yes |
| events and input | initMouseEvent, initUIEvent and initKeyboardEvent are not provided. | #490 | Add the legacy initMouseEvent/initUIEvent/initKeyboardEvent methods. | yes |
| events and input | Clicking summary does not open details. | #498 | Retired by #508 (a click on the summary toggles it). | yes |
| events and input | The onclick property does not read back an inline attribute handler, and inline handlers do not see their form owner in scope. | #498 | Reflect attribute handlers on the property and add the form owner to the handler scope. | yes |
| events and input | Clicking summary does not open details. | #500 | Retired by #508. | yes |
| events and input | A user click on a disabled control is still dispatched (it just does not activate). | #502 | Retired by #521 (the user's click on a disabled control is not dispatched). | yes |
| events and input | toggle does not fire when script changes the open attribute with setAttribute or removeAttribute. | #508 | Fire toggle on attribute changes | yes |
| events and input | Enter or Space on a focused summary does not toggle it. | #508 | Keyboard activation of summary | yes |
| events and input | keyup and keypress are not fired. | #511 | Retired by #515 (a release fires keyup); keypress still open (#515). | yes |
| events and input | KeyboardEvent.code is empty for character keys, and metaKey and repeat are always false. | #511 | Populate code, metaKey and repeat from the host. | yes |
| events and input | beforeinput, change on blur, and focus/blur/focusin/focusout are not fired by typing. | #511 | Dispatch those events. | yes |
| events and input | Keys are not delivered when nothing is focused or when the focused element is not a form control. | #511 | Retired by #515 (keys reach the page with nothing focused). | yes |
| events and input | A deletion's inputType is always deleteContentBackward, forward delete included. | #511 | Report deleteContentForward for forward delete. | yes |
| events and input | Script el.focus() does not move the engine's focus, so typed characters are not inserted into a script-focused field. | #515 | Retired by #517 (the engine and the page share one focus). | yes |
| events and input | No keypress events and no events for modifier keys on their own. | #515 | Fire keypress and record flagsChanged for modifier events. | yes |
| events and input | metaKey is always false and Cmd-key presses are not sent to the page. | #515 | Deliver Cmd-key presses with metaKey set. | yes |
| events and input | KeyboardEvent code is empty for character keys and repeat is always false. | #515 | Populate code and repeat on key events. | yes |
| events and input | No keyup is fired on the window-level key arm. | #515 | Send releases from the window-level key arm to handle_key_up. | yes |
| events and input | Focus moves at the release, not at the press, and a cancelled mousedown does not keep it from moving. | #517 | Press half retired by #525. | yes |
| events and input | Only input, textarea and select take the engine's focus; links, buttons and tabindex elements get no focus ring and no keys. | #517 | Engine focus for links, buttons and tabindex elements | yes |
| events and input | Focus events are not isTrusted and relatedTarget on a click-away blur is null. | #517 | Trusted focus events with relatedTarget | yes |
| events and input | autofocus and Tab navigation are not handled, and :focus in matches/querySelector is unchanged. | #517 | Implement autofocus, Tab navigation and :focus matching | yes |
| events and input | A window losing or gaining keyboard focus does not blur or focus the page. | #517 | Forward window focus changes to the page | yes |
| events and input | unhandledrejection and rejectionhandled are never fired, and form submits do not dispatch SubmitEvent. | #518 | Dispatch rejection events and use SubmitEvent for submit. | yes |
| events and input | :checked on a compound left of a descendant or child combinator is not decided and stays permissive. | #520 | Ancestor state in the selector matcher. | yes |
| events and input | :indeterminate, :default and :focus in script queries are unchanged and do not read live state. | #520 | Live state for these pseudo-classes in script queries. | yes |
| events and input | el.click() from script on a disabled control still dispatches click. | #521 | Make script click() a no-op on disabled controls. | yes |
| events and input | dblclick, contextmenu and auxclick events do not exist. | #521 | Implement dblclick, contextmenu and auxclick. | yes |
| events and input | No pointer or mouse move/hover events are dispatched, so hover menus do not open. | #522 | Retired by #523 (move, over/out, enter/leave). | yes |
| events and input | No dblclick, contextmenu, auxclick, other buttons, modifier keys, movementX/Y or pointer capture. | #522 | Implement dblclick, contextmenu, other buttons, modifiers, movement and pointer capture | yes |
| events and input | screenX/Y are the viewport coordinates. | #522 | Report true screen coordinates | yes |
| events and input | click targets the element under the release point rather than the common ancestor of press and release targets. | #522 | Retired by #526 (common ancestor), corrected by #527. | yes |
| events and input | el.click() from script builds a MouseEvent with detail 0 instead of a PointerEvent. | #522 | Build a PointerEvent for el.click() | yes |
| events and input | offsetX/Y come from the hit-test box, which for text may be the text run's box rather than the element's padding edge. | #522 | Compute offsetX/Y from the target element's padding edge | yes |
| events and input | Mouse events carry no modifier keys. | #523 | Set modifier key state on mouse and pointer events. | yes |
| events and input | CSS :hover does not follow the pointer because the cascade treats nothing as hovered. | #523 | The next slice: hover state in the cascade. | yes |
| events and input | No enter or leave events are sent to the document or the window. | #523 | Fire pointerenter/mouseenter at the document and window. | yes |
| events and input | There is no pointerrawupdate, pointer capture, drag-and-drop or drag text selection. | #523 | pointerrawupdate, pointer capture, drag-and-drop and drag selection. | yes |
| events and input | Moves with a right or other button held are not recorded. | #523 | Record rightMouseDragged and otherMouseDragged in the content view. | yes |
| events and input | offsetX/Y on move and boundary events come from the padding box of the target's first box. | #523 | Compute offsetX/Y against the target's correct box. | yes |
| events and input | Moves collapse to one per loop turn, so movementX/Y is the distance since the last delivered move. | #523 | Deliver every move without collapsing. | yes |
| events and input | screenX/Y are still the viewport coordinates. | #523 | Report true screen coordinates for screenX/Y. | yes |
| events and input | Hover is not re-evaluated when the page scrolls or the layout changes under a still pointer. | #523 | Re-run the hover hit test after scroll and layout changes. | yes |
| events and input | The click after a press and release goes to the element under the release, not to their common ancestor. | #523 | Retired by #526. | yes |
| events and input | Only input, textarea and select take focus at a press; links, buttons and tabindex elements do not. | #525 | Focus links and tabindex elements at a press as Chrome does. | yes |
| events and input | A label's control is focused before click dispatch, and a cancelled click on the label does not stop it. | #525 | Focus the labeled control as the click's activation, after listeners. | yes |
| events and input | click goes to the release's element, not to the common ancestor of the press and release targets. | #525 | Retired by #526. | yes |
| events and input | Focus events are not isTrusted, :focus does not apply in the cascade, and there is no focus ring except the caret. | #525 | Trusted focus events, :focus matching and focus rings. | yes |
| events and input | If a listener removes the pressed element between press and release, its ancestor chain is whatever is left of it. | #526 | Remember the press's ancestor chain robustly against removal | yes |
| events and input | No dblclick, auxclick, or click count above 1. | #526 | Implement click counting, dblclick and auxclick | yes |
| events and input | There is no drag-and-drop, so a press on a link released on another part of it still sends click to the link. | #527 | Implement link drag-and-drop matching Chrome. | yes |
| events and input | In a disabled fieldset or on a disabled button the engine still sends mousedown and mouseup, which Chrome does not. | #527 | Suppress mousedown/mouseup on disabled controls as Chrome does. | yes |
| events and input | A label click focuses its field before the label's click and sends no second click to the field. | #527 | Match Chrome's label click/focus/second-click order. | yes |
| events and input | A press on a link does not focus the link; the engine focuses form controls only. | #527 | Focus links on press. | yes |
| networking | The module-script fetch policy entry point exists but no code path calls it yet. | #448 | The C0 module host wires fetch_module into module loading. | no |
| networking | The script-network bridge adds no XMLHttpRequest or fetch for pages; it is only the seam for them. | #450 | Retired by #455 (XMLHttpRequest) and #459 (fetch). | yes |
| networking | The bridge request queue holds at most 64 requests and refuses further requests with id 0. | #450 | A larger or unbounded queue with backpressure. | yes |
| networking | Requests still pending when the pump rounds or script budget run out complete with a network error. | #450 | A pump that keeps serving requests beyond the bounded rounds and budget. | yes |
| networking | A failure handler that keeps retrying gets three more passes and is then dropped. | #450 | Unbounded retry handling (deliberately bounded today). | yes |
| networking | Every network refusal or failure reaches the page as a generic network error, with the reason only in a debug log. | #450 | None planned; deliberate generic error (would need per-reason page errors). | yes |
| networking | Pages loaded with load_html, or with script_network_enabled off, get no script network bridge. | #450 | A FetchPolicy for load_html pages so they can be given the bridge. | yes |
| networking | The engine tracks no page CSP, so connect-src is not enforced. | #450 | Track the page's CSP and pass it to FetchPolicy. | yes |
| networking | A page on a loopback literal can reach only its own origin for subresources; other loopback hosts are denied. | #453 | None planned; by design of the #445 subresource vet. | yes |
| networking | XMLHttpRequest is asynchronous only; open() with async false throws NotSupportedError. | #455 | Synchronous XHR support without blocking the engine pump | yes |
| networking | XMLHttpRequest has no responseXML or document parsing; responseType document yields null. | #455 | Document parsing for XHR responses (responseXML) | yes |
| networking | XMLHttpRequest sends no cookies; withCredentials is only a hint. | #455 | Cookie support in the script-network fetch policy | yes |
| networking | XMLHttpRequest fires a single progress event because the engine hands over whole bodies. | #455 | Streaming partial response bodies to the page | yes |
| networking | Pages the engine did not enable the network bridge on have no XMLHttpRequest at all. | #455 | Enable the script-network bridge for those pages | yes |
| networking | fetch, Headers, Request and Response are not provided by this change. | #455 | Retired by #459. | yes |
| networking | FetchPolicy::cancel is not called by anything yet, so stop and navigation do not cancel requests. | #456 | Wire cancel() from Engine::stop and from load_url replacing the document. | no |
| networking | Cancellation is document-level only; per-request cancel tokens are not provided. | #456 | Retired by #458 (CancelToken aborts one request). | unknown |
| networking | The cancellable request API is not called by any page code path yet. | #458 | Wire XHR abort() and fetch AbortSignal to execute_cancellable. | no |
| networking | Module fetches cannot be aborted by script. | #458 | Make fetch_module accept a cancel token. | yes |
| networking | fetch response bodies arrive whole, so Response.body is a one-chunk stream with no partial chunks. | #459 | Request/Response body streaming with partial chunks from the engine. | yes |
| networking | Multipart formData() is not parsed and rejects with TypeError. | #459 | Multipart formData() parsing. | yes |
| networking | A request body that is a ReadableStream is read whole before sending. | #459 | Streaming request bodies. | yes |
| networking | The fetch cache, integrity and keepalive options are stored and ignored, and fetch sends no cookies. | #459 | Honour cache/integrity/keepalive and add cookie support. | yes |
| networking | Image requests still carry navigation Sec-Fetch-Dest/Mode and Upgrade-Insecure-Requests defaults. | #470 | Per-destination Sec-Fetch headers in rustkit-http (left for Talos). | no |
| networking | The platform cert loader's errors are still discarded, so the reason an empty root store loads is not logged. | #478 | Log load_native_certs().errors. | no |
| networking | Late fetches run on the UI thread, and a live turn waits at most 2 s on the network, so a slower request fails as a network error. | #486 | Take requests off the UI thread (Z2-C5 scheduler). | yes |
| networking | Images inside a closed details are still fetched. | #508 | Defer fetches for content in closed details | yes |
| script engine | Bare module specifiers such as 'react' are refused because there is no import map support. | #451 | Retired by #468 (import maps and dynamic import()). | yes |
| script engine | Dynamic import() is not part of the module host's first slices. | #451 | Retired by #468. | yes |
| script engine | Dynamic import(), type=importmap and link rel=modulepreload are not supported. | #452 | Import maps and dynamic import() retired by #468; modulepreload not stated as done. | yes |
| script engine | A module whose top-level await outlives the load is polled once and its script element hears load as soon as evaluation starts. | #452 | Poll pending top-level await to completion before firing load. | yes |
| script engine | A module graph is capped at 2,000 modules and 64 levels per page, and anything over the limit fails the whole graph. | #452 | Raise or remove the module graph bounds. | yes |
| script engine | A very large module (1.9 MB on github) fails to parse because of a Boa parser limit. | #452 | Fix the Boa parser limit for large modules. | yes |
| script engine | crypto.getRandomValues, customElements and import maps are not provided by this change. | #459 | crypto.getRandomValues by #483, customElements by #465, import maps by #468. | yes |
| script engine | crypto.getRandomValues and MessageChannel are not provided. | #465 | crypto.getRandomValues retired by #483; MessageChannel still MISSING in the 2026-10-05 census. | yes |
| script engine | The Boa parser fails on github's 1.9 MB landing-pages.js bundle. | #465 | A Boa parser fix or upgrade that parses the bundle. | yes |
| script engine | An import map that arrives after a module has been resolved cannot change what was resolved. | #468 | None stated; spec-aligned limit of import map resolution. | yes |
| script engine | The integrity field of an import map is ignored. | #468 | Honour import map integrity metadata. | yes |
| script engine | External import maps (script type=importmap with src) are not supported. | #468 | None; the spec does not allow it either. | yes |
| script engine | All import maps are read before the first script instead of when each is encountered. | #468 | Register each import map at the point the parser encounters it. | yes |
| script engine | The network/module pump alternates at most four times per call. | #468 | More alternations or a pump that runs until quiescent. | yes |
| script engine | A module fetch refusal rejects the dynamic import with a TypeError and gives the page no detail. | #468 | None planned; deliberate generic rejection. | yes |
| script engine | The Boa parser rejects a github dependency with a strict reserved word let error. | #468 | Reduce the failing construct and fix the Boa parser. | yes |
| script engine | github.com still does not start: landing-pages.js throws an uninitialized-binding ReferenceError while running. | #471 | Fix the remaining run-time throws (TDZ or module-host import ordering, missing platform feature, missing method) | yes |
| script engine | landing-pages.js throws ReferenceError access to uninitialized binding, cause unreduced (Boa TDZ or module-host ordering). | #471 | Reduce and fix the uninitialized-binding error | yes |
| script engine | behaviors.js throws a null/undefined TypeError from a missing platform feature. | #471 | Implement the missing platform feature | yes |
| script engine | github-elements.js throws not a callable function from a missing method not yet probed. | #471 | Identify and implement the missing method | yes |
| script engine | The engine stays on boa_parser 0.20.0 with a vendored patch; the Boa 0.22 upgrade is parked. | #471 | Upgrade to Boa 0.22 and remove the vendored patch | no |
| script engine | Boa prints [native code] for every Function.prototype.toString and Error.stack is undefined. | #472 | Function source text and Error.stack support in the script engine. | yes |
| script engine | A long synchronous timer callback cannot be interrupted because Boa has no wall-clock interrupt. | #472 | A wall-clock interrupt in the script engine. | unknown |
| script engine | RustKitView::process_events() is empty, so timers, promise jobs and network callbacks do not run on the live loop after load. | #480 | Retired by #486 (timers and late fetches run on the live loop). | yes |
| script engine | Boa's JSON.parse rejects lone surrogate escapes. | #482 | Fix lone-surrogate handling in Boa JSON.parse. | yes |
| script engine | crypto.subtle is not provided. | #483 | Implement crypto.subtle. | yes |
| script engine | crypto random bytes come from /dev/urandom, so on non-Unix platforms the call throws OperationError. | #483 | Retired by #495 (crypto on Windows without /dev/urandom). | yes |
| script engine | DOMException has no code property. | #483 | Add the code property to DOMException. | yes |
| script engine | The live loop runs at most 1,000 timer callbacks per turn; the rest wait for the next turn. | #486 | Raise or remove the per-turn timer cap. | yes |
| script engine | The live loop adds at most 2,000 records to a view's script log. | #486 | Raise or remove the live script-log cap. | no |
| script engine | The next live turn waits at least as long as the last one took, so a page that keeps the loop busy gets at most half of it. | #486 | A scheduler that does not halve busy pages' time. | yes |
| script engine | The dynamic import() count restarts every live turn, so the 2,000-module cap is per turn rather than per page. | #486 | Count dynamic imports per page across live turns. | yes |
| script engine | Intervals are not throttled; after the loop is held, a short interval runs many times on the next turn. | #486 | Throttle/coalesce missed interval runs as browsers do. | yes |
| script engine | After a JS engine panic in a live turn, the next turn runs the page again instead of stopping it. | #486 | Stop running a page after a panic in the live loop. | yes |
| script engine | On Windows crypto.getRandomValues and crypto.randomUUID throw OperationError because /dev/urandom does not exist. | #494 | Retired by #495. | yes |
| script engine | Several real-site bootstrap throws remain unfixed (github behaviors.js, apple localeswitcher, cnn inline#11, google csi, github landing-pages TDZ). | #513 | Next batches of bootstrap-throw fixes. | yes |
| script engine | A page whose code loops past the iteration limit inside a promise job still panics the JS engine and poisons the page. | #518 | A fix in Boa, or a wrapper that catches the limit before it reaches the job queue. | yes |
| measurement tools | receipt.py prints chrome_version as null because baselines/metadata.json has no Chrome version key. | #447 | Record the Chrome version in baselines/metadata.json or have receipt.py read it elsewhere | no |
| measurement tools | The real-site interactive column is a design only; no runner exists in this phase. | #466 | Implement the interactive-column measurement pipeline. | no |
| measurement tools | The interactive column is an un-scored diagnostic and does not count toward the board score. | #466 | An A3 decision by Pete to promote it into scoring. | no |
| measurement tools | The layout export emits no box for SVG shapes under a transform, curved paths, text/tspan, use, image/foreignObject, letterboxed fits, non-default object-fit or a missing viewBox. | #467 | Exact geometry modelling for those SVG shape cases in the exporter | no |
| measurement tools | Gate JSON does not record the base SHA (G5 is unbuilt). | #467 | Build G5 to record the base SHA in the gate JSON | no |
| measurement tools | There is no JS-level profiler, so finding what a slow callback does needs source instrumentation. | #472 | A JS-level profiler for Boa. | no |
| measurement tools | The capture tool's --html-file mode does not fetch, so url-background repros need a local server. | #474 | Make --html-file fetch subresources. | no |
| measurement tools | receipt.py reports chrome_version as null because it does not launch Chrome. | #475 | Have receipt.py obtain the Chrome version | no |
| measurement tools | There is no real-window driver to verify clicks in the built app (Z2-I2 does not exist). | #480 | Build the Z2-I2 real-window driver | no |
| measurement tools | The web API census does not measure the engine's cascade selector matcher; selector rows use the bindings' fallback matcher. | #482 | Run the census where rustkit-engine builds, with the engine matcher installed. | no |
| measurement tools | The census does not measure real network responses, layout-dependent values, engine-driven event timing, or macOS/WebKit/GPU-specific behaviour. | #482 | Extend the census harness to a real engine/network/macOS run. | no |
| measurement tools | The census checks only one smoke call per API, and 36 rows check only that the member is callable. | #482 | Deeper conformance checks (e.g. a WPT runner). | no |
| measurement tools | The HAR replay proxy is available only in parity-capture, not in the shipping browser. | #484 | None planned; intentionally test-only. | no |
| measurement tools | CI compiles the macOS set_bounds window test but does not run it. | #487 | Run the real-window viewhost tests in CI. | no |
| measurement tools | CI does not run the real-NSWindow click tests; the engine test needs a display and a GPU. | #499 | Run the macOS click tests in a CI lane with a display and GPU | no |
| measurement tools | The click tests do not exercise -[NSWindow sendEvent:] or the window server, because an off-screen window drops mouse events there. | #499 | A visible-window or hardware-click driver | no |
| measurement tools | The action harness click does not go through the engine's click path, so it sends no pointer or mouse down/up events. | #522 | Z2-I1: route the action harness through mouse_down_at_point/click_at_point | no |
| rendering and images | A data:image/svg+xml CSS background is not discovered and does not paint. | #454 | Retired by #462 (data: SVG images paint as vectors, backgrounds included). | yes |
| rendering and images | SVG backgrounds align as meet, top-left rather than xMidYMid. | #454 | preserveAspectRatio alignment support for SVG images. | yes |
| rendering and images | SVG background tiles are capped at 2500 per background. | #454 | Remove or raise the per-background tile cap. | yes |
| rendering and images | SVG preserveAspectRatio is not parsed. | #462 | Parse and honour preserveAspectRatio in rustkit-svg. | yes |
| rendering and images | rustkit-svg has no gradient paint servers, so fills referencing gradients render nothing. | #462 | Retired by #463 (SVG gradient paint servers fill shapes). | yes |
| rendering and images | SVG gradient fills are painted as a grid of flat-coloured polygons, emitting many fill_polygon commands per shape. | #463 | A gradient-capable polygon command in the renderer | no |
| rendering and images | SVG gradient fills on shapes larger than 64 device px show faint banding and staircased diagonal hard stops. | #463 | A gradient-capable polygon command in the renderer (per-pixel gradient fill) | yes |
| rendering and images | SVG gradient strokes, fx/fy focal points, percentages under userSpaceOnUse, pattern and url() fill fallback colours are not supported. | #463 | Implement gradient strokes, focal points, userSpaceOnUse percentages, pattern and fallback colours | yes |
| rendering and images | Shapes inside SVG defs still render. | #463 | An SVG parser that treats defs content as non-rendered | yes |
| rendering and images | RustKit's SVG renderer does not centre a letterboxed fit the way Chrome's default xMidYMid does. | #467 | Implement preserveAspectRatio xMidYMid centring in render_with_color | yes |
| rendering and images | There is no AVIF decoder, so a server that sends AVIF regardless of Accept still fails. | #470 | Add an AVIF decoder to rustkit-codecs. | yes |
| rendering and images | APNG is advertised but whether it animates or draws only the first frame is unverified. | #470 | Verify or implement APNG animation in rustkit-codecs. | unknown |
| rendering and images | Images load before any script runs instead of overlapping with script execution. | #472 | Overlap image loading with script execution in load_url (D4 image-pipeline lane). | unknown |
| rendering and images | An image added by a timer or callback after the single post-script image pass is not fetched. | #473 | Z2-C5 (scheduler). | yes |
| rendering and images | The post-script image pass gets its own 8 s subresource budget, so a page whose scripts add a stalled image can take up to 8 s longer to capture. | #473 | Share one subresource budget across both image passes. | yes |
| rendering and images | No test covers that an image which already failed is not requested a second time. | #473 | A test server that can serve a failing image. | no |
| rendering and images | Edge-offset and calc() background positions are not parsed. | #474 | Parse edge-offset and calc() background-position values. | yes |
| rendering and images | background-position values in em, rem or viewport units are still treated as the start edge. | #475 | Give the background-position parser font size and viewport context | yes |
| rendering and images | calc() in background-size is not supported. | #475 | Parse calc() in background-size | yes |
| rendering and images | img.width and img.height read the attributes only, with no layout or natural size. | #489 | Back img.width/height with layout and natural size. | yes |
| rendering and images | Element namespaces are only stored; layout and rendering read only tag names. | #490 | Namespace-aware layout and rendering. | yes |
| rendering and images | A Linux build of this tree has no FreeType shaper and uses the 0.5em stub text shaper. | #503 | A FreeType shaper in this tree (hiwave-linux has one). | yes |
| rendering and images | No disclosure triangle is drawn before a summary, so a default summary differs from Chrome by the marker and its indent. | #508 | Z2-D2 list markers | yes |
| rendering and images | document.fonts never loads a font; FontFace.load() resolves but nothing is installed in the engine. | #513 | Install FontFace fonts in the engine. | yes |
| rendering and images | The cursor does not change, so there is no pointer hand for cursor: pointer. | #523 | Apply the CSS cursor property to the native cursor. | yes |
| DOM and parsing | Custom elements support autonomous elements only; customized built-ins via extends or is= are refused or absent. | #465 | Implement customized built-in elements (extends / is=). | yes |
| DOM and parsing | adoptedCallback is not provided because there is only one document. | #465 | Multi-document support with adoptedCallback. | yes |
| DOM and parsing | Setting a.href does not set the href attribute; other reflecting properties are unchecked. | #480 | Reflect href (and other reflecting properties) to attributes | yes |
| DOM and parsing | The HTML parser drops text-only input before or after head when there is no body content. | #481 | Fix rustkit-html handling of text with no body content | yes |
| DOM and parsing | getAttributeNames returns names sorted, not in source order. | #481 | Store attributes in source order in rustkit-dom | yes |
| DOM and parsing | DOMParser does not parse XML types; they throw NotSupportedError. | #481 | XML parsing in DOMParser | yes |
| DOM and parsing | createElement on a DOMParser document creates nodes owned by the global document. | #481 | Per-document node ownership for parsed documents | yes |
| DOM and parsing | querySelector on a DOMParser document without the injected matcher only searches the bound document. | #481 | Make the fallback selector search the parsed document | unknown |
| DOM and parsing | Template contents are not in a separate inert document; their ownerDocument is the global document. | #481 | Give template contents a separate inert document | yes |
| DOM and parsing | None of the 10 probed Range and Selection APIs and none of the 6 parsing and serialization APIs work. | #482 | DOMParser text/html now works (census 2026-10-05); Range and Selection still 0 of 10. | yes |
| DOM and parsing | Attributes are stored without a namespace, so *AttributeNS methods resolve only the known xlink, xml and xmlns prefixes. | #490 | Namespace-aware attribute storage in rustkit-dom. | yes |
| DOM and parsing | A prefixed createElementNS name keeps the prefix in localName, and a null-namespace element is still case-folded as HTML. | #490 | Proper prefix/localName split and null-namespace handling. | yes |
| DOM and parsing | SVG parsed from HTML stays in the HTML namespace because the parser assigns no foreign namespaces. | #490 | Foreign-content namespace assignment in the HTML parser. | yes |
| DOM and parsing | An Attr keeps its object identity per name even after the attribute is removed and re-added. | #490 | Create a new Attr object when an attribute is removed and re-added. | yes |
| DOM and parsing | document.forms/images/links and similar collections are snapshots taken when read, not live. | #513 | Live document collections. | yes |
| DOM and parsing | document.visibilityState is always visible. | #513 | Report real page visibility. | yes |
| view host and app shell | ViewHost::set_visible has no macOS arm, so it never hides the NSView. | #487 | A macOS arm for ViewHost::set_visible. | yes |
| view host and app shell | The shell discards set_bounds results, so a failed resize is silent. | #487 | Handle or log the set_bounds result in apply_layout. | no |
| view host and app shell | On a 2x Retina display the page is drawn at half the display resolution and stretched. | #487 | Size the drawable in device pixels using backingScaleFactor. | yes |

## How to keep this current

- **Each new PR adds its own limits here, in the same PR.** If a PR body has a
  "Stated limits" section, or says something "does not" or is "not implemented",
  add one row per limit. Use the PR's area, one plain sentence, the PR number,
  what would retire it, and whether a page can observe it.
- **A PR that retires a limit edits that row.** Keep the row and set its
  "What would retire it" cell to `Retired by #N (...)`. Do not delete it.
- **Do not add a row that no PR body states.** A limit found by measurement goes
  into the web API census, not here, until a PR states it.
- Rebuild the set from GitHub when in doubt. List merged PRs into `develop`
  with `gh pr list --state merged --base develop --limit 200 --json number,title,body,mergedAt`,
  or with the REST endpoint `repos/{owner}/{repo}/pulls?state=closed&base=develop`,
  then read each body for limit statements.
