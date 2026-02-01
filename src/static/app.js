(function() {
    'use strict';

    const DEBUG = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';

    function log(...args) {
        if (DEBUG) {
            console.log('[sorcery]', ...args);
            const debugInfo = document.getElementById('debug-info');
            if (debugInfo) {
                debugInfo.style.display = 'block';
                debugInfo.textContent += args.join(' ') + '\n';
            }
        }
    }

    function showError(message) {
        const spinner = document.getElementById('spinner');
        const status = document.getElementById('status');
        const messageEl = document.getElementById('message');
        const errorContainer = document.getElementById('error-container');
        const errorMessage = document.getElementById('error-message');

        spinner.style.display = 'none';
        status.textContent = 'Unable to Open';
        messageEl.style.display = 'none';
        errorContainer.style.display = 'block';
        errorMessage.textContent = message;
    }

    function parseAtLineSuffix(target) {
        const lower = target.toLowerCase();
        let markerIndex = target.lastIndexOf('@');
        let markerLen = 1;
        const encodedIndex = lower.lastIndexOf('%40');
        if (encodedIndex > markerIndex) {
            markerIndex = encodedIndex;
            markerLen = 3;
        }
        if (markerIndex === -1) return null;

        const lastSlash = target.lastIndexOf('/');
        if (markerIndex < lastSlash) return null;

        const suffix = target.slice(markerIndex + markerLen);
        if (!suffix) return null;
        if (suffix[0].toLowerCase() !== 'l') return null;

        const rest = suffix.slice(1);
        if (rest.length === 0) {
            return { path: target.slice(0, markerIndex), line: null, column: null };
        }

        let i = 0;
        while (i < rest.length && rest[i] >= '0' && rest[i] <= '9') {
            i += 1;
        }

        if (i > 0) {
            const line = parseInt(rest.slice(0, i), 10);
            const rem = rest.slice(i);
            if (!rem) {
                return { path: target.slice(0, markerIndex), line, column: null };
            }
            const marker = rem[0];
            if (marker === 'c' || marker === 'C' || marker === ':') {
                const colTail = rem.slice(1);
                if (colTail.length === 0) {
                    return { path: target.slice(0, markerIndex), line, column: null };
                }
                if (/^\d+$/.test(colTail)) {
                    const column = parseInt(colTail, 10);
                    return { path: target.slice(0, markerIndex), line, column: column <= 120 ? column : null };
                }
            }
            return null;
        }

        const marker = rest[0];
        if (marker === 'c' || marker === 'C') {
            const colTail = rest.slice(1);
            if (colTail.length === 0 || /^\d+$/.test(colTail)) {
                return { path: target.slice(0, markerIndex), line: null, column: null };
            }
        }

        return null;
    }

    function parseSorceryPayload(raw) {
        log('Parsing payload:', raw);

        const [targetPart, queryPart] = raw.split('?', 2);
        let target = targetPart;
        const query = queryPart || '';

        let lineAt = null;
        let columnAt = null;
        const atParsed = parseAtLineSuffix(target);
        if (atParsed) {
            lineAt = atParsed.line;
            columnAt = atParsed.column;
            target = atParsed.path;
            log('Extracted @L-style line:', lineAt, 'column:', columnAt);
        }

        let lineGithub = null;
        const mGithub = target.match(/#L(\d+)(?:-L?\d+)?$/);
        if (mGithub) {
            lineGithub = parseInt(mGithub[1], 10);
            target = target.substring(0, mGithub.index);
            log('Extracted GitHub-style line:', lineGithub);
        }

        let lineColon = null;
        let columnColon = null;
        if (!atParsed) {
            const mColon = target.match(/:(\d+)(?::(\d+))?$/);
            if (mColon) {
                lineColon = parseInt(mColon[1], 10);
                if (mColon[2]) {
                    columnColon = parseInt(mColon[2], 10);
                }
                target = target.substring(0, mColon.index);
                log('Extracted colon-style line:', lineColon, 'column:', columnColon);
            }
        }

        const line = lineAt ?? lineColon ?? lineGithub ?? null;
        const column = columnAt ?? columnColon ?? null;

        const isAbsolute = target.startsWith('//');
        const path = isAbsolute ? target.substring(2) : target;

        log('Parsed:', { isAbsolute, path, line, column, query });

        return { isAbsolute, path, line, column, query };
    }

    function buildCustomProtocol(parsed) {
        let protocolUrl;

        if (parsed.isAbsolute) {
            // Absolute path: srcuri://abs/path/to/file (authority-based v1 spec)
            protocolUrl = `srcuri://abs/${parsed.path}`;
        } else {
            // Workspace path: srcuri://repo/path (workspace name IS authority, v1 spec)
            protocolUrl = `srcuri://${parsed.path}`;
        }

        if (parsed.line !== null) {
            protocolUrl += `@L${parsed.line}`;
            if (parsed.column !== null) {
                protocolUrl += `C${parsed.column}`;
            }
        }

        if (parsed.query) {
            protocolUrl += `?${parsed.query}`;
        }

        log('Built protocol URL:', protocolUrl);
        return protocolUrl;
    }

    function attemptOpen() {
        try {
            const hash = window.location.hash;
            log('Raw hash:', hash);

            if (!hash || hash.length <= 1) {
                showError('No file path provided in the URL. Expected format: #path/to/file@L42?workspace=name');
                return;
            }

            const payload = hash.substring(1);
            log('Payload:', payload);

            const parsed = parseSorceryPayload(payload);

            if (!parsed.path) {
                showError('Invalid path in URL');
                return;
            }

            const protocolUrl = buildCustomProtocol(parsed);
            log('Redirecting to:', protocolUrl);

            window.location.href = protocolUrl;

            setTimeout(() => {
                showError(
                    'The sorcery protocol handler is not installed or not responding. ' +
                    'Please install Sorcery Desktop to open links directly in your editor.'
                );
            }, 3000);

        } catch (error) {
            log('Error:', error);
            showError(`Error processing URL: ${error.message}`);
        }
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', attemptOpen);
    } else {
        attemptOpen();
    }
})();
