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

    function parseSorceryPayload(raw) {
        log('Parsing payload:', raw);

        const [targetPart, queryPart] = raw.split('?', 2);
        let target = targetPart;
        const query = queryPart || '';

        let lineGithub = null;
        const mGithub = target.match(/#L(\d+)(?:-L?\d+)?$/);
        if (mGithub) {
            lineGithub = parseInt(mGithub[1], 10);
            target = target.substring(0, mGithub.index);
            log('Extracted GitHub-style line:', lineGithub);
        }

        let lineColon = null;
        let columnColon = null;
        const mColon = target.match(/:(\d+)(?::(\d+))?$/);
        if (mColon) {
            lineColon = parseInt(mColon[1], 10);
            if (mColon[2]) {
                columnColon = parseInt(mColon[2], 10);
            }
            target = target.substring(0, mColon.index);
            log('Extracted colon-style line:', lineColon, 'column:', columnColon);
        }

        const line = lineColon ?? lineGithub ?? null;
        const column = columnColon ?? null;

        const isAbsolute = target.startsWith('//');
        const path = isAbsolute ? target.substring(2) : target;

        log('Parsed:', { isAbsolute, path, line, column, query });

        return { isAbsolute, path, line, column, query };
    }

    function buildCustomProtocol(parsed) {
        let protocolUrl;

        if (parsed.isAbsolute) {
            protocolUrl = `srcuri:///${parsed.path}`;
        } else {
            protocolUrl = `srcuri://${parsed.path}`;
        }

        if (parsed.line !== null) {
            protocolUrl += `:${parsed.line}`;
            if (parsed.column !== null) {
                protocolUrl += `:${parsed.column}`;
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
                showError('No file path provided in the URL. Expected format: #path/to/file:line?workspace=name');
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
