// Inspector injection script - injected into content WebView
// Note: Console interception is handled separately in console_inject.js
(function() {
    'use strict';

    // Prevent multiple injections
    if (window.__hiwaveInspector) return;

    // Highlight overlay element
    let highlightOverlay = null;
    let pickerActive = false;
    let lastHighlightedElement = null;

    // Create highlight overlay
    function createHighlightOverlay() {
        if (highlightOverlay) return highlightOverlay;

        highlightOverlay = document.createElement('div');
        highlightOverlay.id = '__hiwave-inspector-highlight';
        highlightOverlay.style.cssText = `
            position: fixed;
            pointer-events: none;
            z-index: 2147483647;
            background: rgba(66, 133, 244, 0.2);
            border: 2px solid rgba(66, 133, 244, 0.8);
            border-radius: 2px;
            transition: all 0.1s ease-out;
            display: none;
        `;

        // Tooltip for element info
        const tooltip = document.createElement('div');
        tooltip.id = '__hiwave-inspector-tooltip';
        tooltip.style.cssText = `
            position: absolute;
            bottom: 100%;
            left: 0;
            background: #1e1e1e;
            color: #fff;
            font-family: -apple-system, BlinkMacSystemFont, 'SF Mono', monospace;
            font-size: 11px;
            padding: 4px 8px;
            border-radius: 4px;
            white-space: nowrap;
            margin-bottom: 4px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.3);
        `;
        highlightOverlay.appendChild(tooltip);

        document.body.appendChild(highlightOverlay);
        return highlightOverlay;
    }

    // Get unique selector path for element
    function getElementPath(element) {
        if (!element || element === document.body) return 'body';
        if (element === document.documentElement) return 'html';
        if (element.nodeType !== Node.ELEMENT_NODE) return '';

        const parts = [];
        let current = element;

        while (current && current !== document.body && current.nodeType === Node.ELEMENT_NODE) {
            let selector = current.tagName.toLowerCase();

            if (current.id) {
                selector += '#' + CSS.escape(current.id);
                parts.unshift(selector);
                break; // ID is unique, stop here
            } else {
                // Add class names (first 2)
                const classes = Array.from(current.classList).slice(0, 2);
                if (classes.length > 0) {
                    selector += '.' + classes.map(c => CSS.escape(c)).join('.');
                }

                // Add nth-child if needed for uniqueness
                const parent = current.parentElement;
                if (parent) {
                    const siblings = Array.from(parent.children).filter(
                        el => el.tagName === current.tagName
                    );
                    if (siblings.length > 1) {
                        const index = siblings.indexOf(current) + 1;
                        selector += `:nth-child(${index})`;
                    }
                }
            }

            parts.unshift(selector);
            current = current.parentElement;
        }

        return 'body > ' + parts.join(' > ');
    }

    // Serialize DOM tree to JSON
    function serializeDomTree(node, maxDepth = 10, currentDepth = 0) {
        if (!node || currentDepth > maxDepth) return null;

        // Skip inspector overlay
        if (node.id === '__hiwave-inspector-highlight') return null;

        const result = {
            type: getNodeType(node),
            path: node.nodeType === Node.ELEMENT_NODE ? getElementPath(node) : ''
        };

        if (node.nodeType === Node.ELEMENT_NODE) {
            result.tag = node.tagName.toLowerCase();
            result.id = node.id || null;
            result.classes = Array.from(node.classList);

            // Get attributes (excluding some verbose ones)
            const excludeAttrs = ['class', 'id', 'style'];
            const attrs = {};
            for (const attr of node.attributes) {
                if (!excludeAttrs.includes(attr.name)) {
                    attrs[attr.name] = attr.value.length > 100
                        ? attr.value.substring(0, 100) + '...'
                        : attr.value;
                }
            }
            if (Object.keys(attrs).length > 0) {
                result.attributes = attrs;
            }

            // Get children
            const children = [];
            for (const child of node.childNodes) {
                // Skip empty text nodes and comments
                if (child.nodeType === Node.TEXT_NODE) {
                    const text = child.textContent.trim();
                    if (text) {
                        children.push({
                            type: 'text',
                            textContent: text.substring(0, 200),
                            path: ''
                        });
                    }
                } else if (child.nodeType === Node.ELEMENT_NODE) {
                    const serialized = serializeDomTree(child, maxDepth, currentDepth + 1);
                    if (serialized) {
                        children.push(serialized);
                    }
                } else if (child.nodeType === Node.COMMENT_NODE) {
                    children.push({
                        type: 'comment',
                        textContent: child.textContent.substring(0, 100),
                        path: ''
                    });
                }
            }

            if (children.length > 0) {
                result.children = children;
            }

            // Get text content for leaf nodes
            if (children.length === 0) {
                const text = node.textContent.trim();
                if (text) {
                    result.textContent = text.substring(0, 200);
                }
            }
        } else if (node.nodeType === Node.TEXT_NODE) {
            result.textContent = node.textContent.trim().substring(0, 200);
        } else if (node.nodeType === Node.COMMENT_NODE) {
            result.textContent = node.textContent.substring(0, 100);
        }

        return result;
    }

    function getNodeType(node) {
        switch (node.nodeType) {
            case Node.ELEMENT_NODE: return 'element';
            case Node.TEXT_NODE: return 'text';
            case Node.COMMENT_NODE: return 'comment';
            case Node.DOCUMENT_NODE: return 'document';
            default: return 'unknown';
        }
    }

    // Get element by path
    function getElementByPath(path) {
        if (!path) return null;
        try {
            return document.querySelector(path);
        } catch (e) {
            console.warn('Invalid selector path:', path);
            return null;
        }
    }

    // Get computed styles for element
    function getElementStyles(element) {
        if (!element) return null;

        const computed = window.getComputedStyle(element);
        const rect = element.getBoundingClientRect();

        // Important style properties to return
        const importantProps = [
            'display', 'position', 'top', 'right', 'bottom', 'left',
            'width', 'height', 'min-width', 'max-width', 'min-height', 'max-height',
            'margin', 'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
            'padding', 'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
            'border', 'border-width', 'border-style', 'border-color',
            'border-top-width', 'border-right-width', 'border-bottom-width', 'border-left-width',
            'background', 'background-color', 'background-image',
            'color', 'font-family', 'font-size', 'font-weight', 'line-height',
            'text-align', 'text-decoration', 'text-transform',
            'flex', 'flex-direction', 'flex-wrap', 'justify-content', 'align-items',
            'grid', 'grid-template-columns', 'grid-template-rows', 'gap',
            'opacity', 'visibility', 'overflow', 'z-index',
            'transform', 'transition', 'box-shadow', 'border-radius'
        ];

        const styles = {};
        for (const prop of importantProps) {
            const value = computed.getPropertyValue(prop);
            if (value && value !== 'none' && value !== 'normal' && value !== 'auto') {
                styles[prop] = value;
            }
        }

        // Box model values
        const boxModel = {
            width: rect.width,
            height: rect.height,
            marginTop: parseFloat(computed.marginTop) || 0,
            marginRight: parseFloat(computed.marginRight) || 0,
            marginBottom: parseFloat(computed.marginBottom) || 0,
            marginLeft: parseFloat(computed.marginLeft) || 0,
            paddingTop: parseFloat(computed.paddingTop) || 0,
            paddingRight: parseFloat(computed.paddingRight) || 0,
            paddingBottom: parseFloat(computed.paddingBottom) || 0,
            paddingLeft: parseFloat(computed.paddingLeft) || 0,
            borderTop: parseFloat(computed.borderTopWidth) || 0,
            borderRight: parseFloat(computed.borderRightWidth) || 0,
            borderBottom: parseFloat(computed.borderBottomWidth) || 0,
            borderLeft: parseFloat(computed.borderLeftWidth) || 0
        };

        return {
            selector: getElementPath(element),
            tagName: element.tagName.toLowerCase(),
            id: element.id || null,
            classes: Array.from(element.classList),
            rect: {
                top: rect.top,
                left: rect.left,
                width: rect.width,
                height: rect.height
            },
            boxModel: boxModel,
            computedStyles: styles
        };
    }

    // Highlight element
    function highlightElement(element) {
        const overlay = createHighlightOverlay();

        if (!element) {
            overlay.style.display = 'none';
            lastHighlightedElement = null;
            return;
        }

        lastHighlightedElement = element;
        const rect = element.getBoundingClientRect();

        overlay.style.display = 'block';
        overlay.style.top = rect.top + 'px';
        overlay.style.left = rect.left + 'px';
        overlay.style.width = rect.width + 'px';
        overlay.style.height = rect.height + 'px';

        // Update tooltip
        const tooltip = overlay.querySelector('#__hiwave-inspector-tooltip');
        if (tooltip) {
            let info = element.tagName.toLowerCase();
            if (element.id) info += '#' + element.id;
            if (element.classList.length > 0) {
                info += '.' + Array.from(element.classList).slice(0, 2).join('.');
            }
            info += ` | ${Math.round(rect.width)} x ${Math.round(rect.height)}`;
            tooltip.textContent = info;

            // Position tooltip above or below
            if (rect.top < 30) {
                tooltip.style.bottom = 'auto';
                tooltip.style.top = '100%';
                tooltip.style.marginBottom = '0';
                tooltip.style.marginTop = '4px';
            } else {
                tooltip.style.bottom = '100%';
                tooltip.style.top = 'auto';
                tooltip.style.marginTop = '0';
                tooltip.style.marginBottom = '4px';
            }
        }
    }

    // Picker mode handlers
    function handlePickerMouseMove(e) {
        if (!pickerActive) return;

        const element = document.elementFromPoint(e.clientX, e.clientY);
        if (element && element.id !== '__hiwave-inspector-highlight' &&
            !element.closest('#__hiwave-inspector-highlight')) {
            highlightElement(element);
        }
    }

    function handlePickerClick(e) {
        if (!pickerActive) return;

        e.preventDefault();
        e.stopPropagation();

        const element = document.elementFromPoint(e.clientX, e.clientY);
        if (element && element.id !== '__hiwave-inspector-highlight' &&
            !element.closest('#__hiwave-inspector-highlight')) {

            // Send selected element to inspector
            const path = getElementPath(element);
            const styles = getElementStyles(element);

            window.ipc.postMessage(JSON.stringify({
                cmd: 'inspector_element_picked',
                path: path,
                styles: styles
            }));

            // Stop picker mode
            stopPicker();
        }
    }

    function startPicker() {
        pickerActive = true;
        document.addEventListener('mousemove', handlePickerMouseMove, true);
        document.addEventListener('click', handlePickerClick, true);
        document.body.style.cursor = 'crosshair';
    }

    function stopPicker() {
        pickerActive = false;
        document.removeEventListener('mousemove', handlePickerMouseMove, true);
        document.removeEventListener('click', handlePickerClick, true);
        document.body.style.cursor = '';

        // Hide highlight
        if (highlightOverlay) {
            highlightOverlay.style.display = 'none';
        }

        window.ipc.postMessage(JSON.stringify({
            cmd: 'inspector_picker_stopped'
        }));
    }

    // Public API
    window.__hiwaveInspector = {
        getDomTree: function(maxDepth = 10) {
            const tree = serializeDomTree(document.documentElement, maxDepth);
            return tree;
        },

        getElementStyles: function(path) {
            const element = getElementByPath(path);
            return getElementStyles(element);
        },

        highlightElement: function(path) {
            const element = path ? getElementByPath(path) : null;
            highlightElement(element);
        },

        clearHighlight: function() {
            highlightElement(null);
        },

        startPicker: function() {
            startPicker();
        },

        stopPicker: function() {
            stopPicker();
        },

        isPickerActive: function() {
            return pickerActive;
        },

        // Get element at specific coordinates
        getElementAtPoint: function(x, y) {
            const element = document.elementFromPoint(x, y);
            if (element) {
                return {
                    path: getElementPath(element),
                    styles: getElementStyles(element)
                };
            }
            return null;
        },

        // Scroll element into view
        scrollToElement: function(path) {
            const element = getElementByPath(path);
            if (element) {
                element.scrollIntoView({ behavior: 'smooth', block: 'center' });
            }
        },

        // Cleanup
        destroy: function() {
            stopPicker();
            if (highlightOverlay && highlightOverlay.parentNode) {
                highlightOverlay.parentNode.removeChild(highlightOverlay);
            }
            highlightOverlay = null;
            delete window.__hiwaveInspector;
        }
    };

    // Signal that inspector is ready
    window.ipc.postMessage(JSON.stringify({
        cmd: 'inspector_inject_ready'
    }));
})();
