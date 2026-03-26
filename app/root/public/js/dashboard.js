import { initTheme, toggleTheme } from './theme.js';

const DEFAULT_REFRESH_INTERVAL_SECS = 10;
const MAX_ACTIVITY_ITEMS = 18;

const state = {
    snapshot: null,
    activities: [],
    lastFetchedAt: null,
    pollTimer: null,
    tickTimer: null,
    isLoading: false,
    eventSource: null,
    streamConnected: false,
};

document.addEventListener('DOMContentLoaded', () => {
    initTheme();
    bindThemeToggle();
    setConsoleState('Connecting');
    fetchSnapshot();
    state.tickTimer = window.setInterval(updateRelativeTimes, 1000);
});

function bindThemeToggle() {
    const button = document.getElementById('themeToggle');
    if (!button) {
        return;
    }
    button.addEventListener('click', toggleTheme);
}

async function fetchSnapshot() {
    if (state.isLoading) {
        return;
    }

    state.isLoading = true;
    setConsoleState(state.snapshot ? buildConsoleState('Refreshing') : 'Connecting');

    try {
        const response = await fetch('/api/admin/dashboard/overview', {
            credentials: 'same-origin',
            headers: {
                'Accept': 'application/json',
                'Cache-Control': 'no-cache',
            },
        });

        if (response.status === 401) {
            window.location.href = '/';
            return;
        }

        if (!response.ok) {
            throw new Error(`Dashboard request failed with ${response.status}`);
        }

        const snapshot = await response.json();
        state.snapshot = snapshot;
        state.activities = Array.isArray(snapshot.activities) ? [...snapshot.activities] : [];
        state.lastFetchedAt = new Date();

        renderSnapshot(snapshot);
        schedulePolling(snapshot.refresh_interval_secs || DEFAULT_REFRESH_INTERVAL_SECS);
        ensureActivityStream();
        setConsoleState(buildConsoleState('Live'));
        updateRelativeTimes();
    } catch (error) {
        console.error('Failed to load dashboard snapshot', error);
        setConsoleState(buildConsoleState('Retrying'));

        if (!state.snapshot) {
            renderGlobalError('Unable to load the admin dashboard snapshot.');
        }
    } finally {
        state.isLoading = false;
    }
}

function schedulePolling(intervalSecs) {
    const safeIntervalSecs = Math.max(5, Number(intervalSecs) || DEFAULT_REFRESH_INTERVAL_SECS);
    const cadenceElement = document.getElementById('refreshCadence');
    if (cadenceElement) {
        cadenceElement.textContent = `${safeIntervalSecs}s polling`;
    }

    if (state.pollTimer) {
        clearInterval(state.pollTimer);
    }

    state.pollTimer = window.setInterval(fetchSnapshot, safeIntervalSecs * 1000);
}

function ensureActivityStream() {
    if (state.eventSource) {
        return;
    }

    const source = new EventSource('/api/admin/dashboard/activity-stream', { withCredentials: true });
    state.eventSource = source;

    source.addEventListener('open', () => {
        state.streamConnected = true;
        setConsoleState(buildConsoleState('Live'));
    });

    source.addEventListener('activity', (event) => {
        try {
            const payload = JSON.parse(event.data);
            prependActivity(payload);
            setConsoleState(buildConsoleState('Live'));
        } catch (error) {
            console.error('Failed to parse dashboard activity payload', error);
        }
    });

    source.onerror = () => {
        state.streamConnected = false;
        setConsoleState(buildConsoleState('Stream reconnecting'));
    };
}

function prependActivity(item) {
    if (!item || !item.kind || !item.label) {
        return;
    }

    const identity = activityIdentity(item);
    state.activities = [item, ...state.activities.filter((entry) => activityIdentity(entry) !== identity)];
    state.activities = state.activities.slice(0, MAX_ACTIVITY_ITEMS);
    renderActivities(state.activities);
    updateRelativeTimes();
}

function renderSnapshot(snapshot) {
    renderMetrics(snapshot.overview);
    renderTrendChart(snapshot.trends || []);
    renderStorage(snapshot.disk);
    renderSystem(snapshot.system, snapshot.disk);
    renderOperations(snapshot.operations || {});
    renderActivities(state.activities);
    renderRecentUploads(snapshot.recent_uploads || []);
    renderCrawlerSummaries(snapshot.crawler_summaries || []);
    renderCrawlerSurfaceSummaries(snapshot.crawler_surface_summaries || []);
    renderCrawlerTopPaths(snapshot.crawler_top_paths || []);
    renderCrawlerHits(snapshot.recent_crawler_hits || []);
    renderUploadPipeline(snapshot.upload_jobs || []);
    renderAlerts(snapshot.alerts || []);
    renderAdminAudit(snapshot.admin_audit || []);
    renderTopTags(snapshot.top_tags || []);
}

function renderMetrics(overview) {
    const container = document.getElementById('metricsGrid');
    if (!container) {
        return;
    }

    const metricCards = [
        {
            label: 'Total Images',
            value: formatNumber(overview.total_images),
            foot: 'Current gallery footprint',
            accent: 'accent-cyan',
        },
        {
            label: 'Uploads Today',
            value: formatNumber(overview.uploads_today),
            foot: 'Fresh media pushed today',
            accent: 'accent-green',
        },
        {
            label: 'Comments Today',
            value: formatNumber(overview.comments_today),
            foot: 'Conversation velocity',
            accent: 'accent-amber',
        },
        {
            label: 'Likes Today',
            value: formatNumber(overview.likes_today),
            foot: 'Engagement coming in',
            accent: 'accent-rose',
        },
        {
            label: 'Unique Tags',
            value: formatNumber(overview.unique_tags),
            foot: 'Searchable topical surface',
            accent: 'accent-cyan',
        },
        {
            label: 'Active Users 24h',
            value: formatNumber(overview.active_users_24h),
            foot: 'Recently active accounts',
            accent: 'accent-green',
        },
        {
            label: 'Open Sessions',
            value: formatNumber(overview.active_sessions),
            foot: 'Unexpired admin sessions',
            accent: 'accent-amber',
        },
        {
            label: 'Pending Uploads',
            value: formatNumber(overview.pending_uploads),
            foot: 'Items still processing',
            accent: 'accent-rose',
        },
    ];

    container.innerHTML = metricCards.map((metric) => `
        <article class="metric-card">
            <div class="metric-label">${escapeHtml(metric.label)}</div>
            <div class="metric-value ${metric.accent}">${escapeHtml(metric.value)}</div>
            <div class="metric-foot">${escapeHtml(metric.foot)}</div>
        </article>
    `).join('');
}

function renderTrendChart(trends) {
    const container = document.getElementById('trendChart');
    if (!container) {
        return;
    }

    if (!trends.length) {
        container.innerHTML = '<div class="empty-panel">No activity data available yet.</div>';
        return;
    }

    const peak = Math.max(
        ...trends.flatMap((point) => [point.uploads, point.comments, point.likes]),
        1
    );

    container.innerHTML = trends.map((point) => {
        const uploadsHeight = scaledBarHeight(point.uploads, peak);
        const commentsHeight = scaledBarHeight(point.comments, peak);
        const likesHeight = scaledBarHeight(point.likes, peak);
        const title = `${point.label} | uploads ${point.uploads}, comments ${point.comments}, likes ${point.likes}`;

        return `
            <div class="trend-day" title="${escapeHtml(title)}">
                <div class="trend-bars">
                    <div class="trend-bar uploads" style="height:${uploadsHeight}%"></div>
                    <div class="trend-bar comments" style="height:${commentsHeight}%"></div>
                    <div class="trend-bar likes" style="height:${likesHeight}%"></div>
                </div>
                <div class="trend-label">${escapeHtml(point.label)}</div>
            </div>
        `;
    }).join('');
}

function renderStorage(disk) {
    const rail = document.getElementById('storageRail');
    const legend = document.getElementById('storageLegend');
    if (!rail || !legend) {
        return;
    }

    const total = Math.max(disk.total_bytes || 0, 1);
    const uploadsPercent = clampPercentage((disk.uploads_bytes / total) * 100);
    const othersPercent = clampPercentage((disk.others_bytes / total) * 100);
    const freePercent = clampPercentage(100 - uploadsPercent - othersPercent);

    rail.innerHTML = `
        <div class="storage-segment segment-uploads" style="width:${uploadsPercent}%"></div>
        <div class="storage-segment segment-others" style="width:${othersPercent}%"></div>
        <div class="storage-segment segment-free" style="width:${freePercent}%"></div>
    `;

    const rows = [
        { label: 'Uploads', value: `${disk.uploads} (${uploadsPercent.toFixed(1)}%)`, colorClass: 'segment-uploads' },
        { label: 'Others', value: `${disk.others} (${othersPercent.toFixed(1)}%)`, colorClass: 'segment-others' },
        { label: 'Free', value: `${disk.free} (${freePercent.toFixed(1)}%)`, colorClass: 'segment-free' },
        { label: 'Used', value: `${disk.used_percent.toFixed(1)}% of ${disk.total}`, colorClass: 'segment-others' },
    ];

    legend.innerHTML = rows.map((row) => `
        <div class="kv-row">
            <div class="kv-label">
                <span class="legend-pill">
                    <span class="legend-swatch ${row.colorClass}"></span>
                    ${escapeHtml(row.label)}
                </span>
            </div>
            <div class="kv-value">${escapeHtml(row.value)}</div>
        </div>
    `).join('');
}

function renderSystem(system, disk) {
    const container = document.getElementById('systemList');
    if (!container) {
        return;
    }

    const rows = [
        { label: 'App uptime', value: formatDuration(system.app_uptime_seconds) },
        { label: 'DB ping', value: `${formatNumber(system.db_latency_ms)} ms` },
        { label: 'Total comments', value: formatNumber(system.total_comments) },
        { label: 'Total likes', value: formatNumber(system.total_likes) },
        { label: 'Disk used bytes', value: formatNumber(disk.used_bytes) },
        { label: 'Latest upload', value: formatTimestamp(system.latest_upload_at) },
        { label: 'Latest comment', value: formatTimestamp(system.latest_comment_at) },
    ];

    container.innerHTML = rows.map((row) => `
        <div class="kv-row">
            <div class="kv-label">${escapeHtml(row.label)}</div>
            <div class="kv-value">${escapeHtml(row.value)}</div>
        </div>
    `).join('');
}

function renderOperations(operations) {
    const summaryContainer = document.getElementById('opsSummary');
    const historyContainer = document.getElementById('rebuildHistory');

    if (summaryContainer) {
        const backup = operations.latest_backup;
        const rehearsal = operations.latest_rehearsal;
        const maintenance = operations.latest_maintenance;
        const smoke = operations.latest_smoke_check;
        const rollback = operations.latest_rollback;
        const latestAlert = operations.latest_alert_delivery;
        const alertSummary = operations.alert_delivery_summary || {};
        const crawler = operations.crawler_issues || {};

        const cards = [
            {
                title: 'Latest backup',
                status: backup?.status || 'unavailable',
                lines: backup ? [
                    formatRelativeTime(backup.recorded_at),
                    `${formatNumber(backup.uploads_original_count)} originals · ${formatNumber(backup.uploads_thumb_count)} thumbs`,
                    formatOpsCounts(backup.db_summary?.table_counts),
                ] : ['No backup status file yet.'],
            },
            {
                title: 'Latest restore rehearsal',
                status: rehearsal?.status || 'unavailable',
                lines: rehearsal ? [
                    formatRelativeTime(rehearsal.recorded_at),
                    `${formatNumber(rehearsal.uploads_original_count)} originals · ${formatNumber(rehearsal.uploads_thumb_count)} thumbs`,
                    formatOpsCounts(rehearsal.db_rehearsal?.rehearsal_counts || rehearsal.db_rehearsal?.table_counts),
                ] : ['No rehearsal status file yet.'],
            },
            {
                title: 'Ops maintenance',
                status: maintenance?.status || 'unavailable',
                lines: maintenance ? [
                    formatRelativeTime(maintenance.recorded_at),
                    `alerts ${formatNumber(maintenance.alerts_lines)} · audit ${formatNumber(maintenance.admin_audit_lines)} · rebuild ${formatNumber(maintenance.rebuild_history_lines)}`,
                    `crawler ${formatNumber(maintenance.crawler_hit_lines || 0)} · smoke ${formatNumber(maintenance.smoke_history_lines || 0)} · rollback ${formatNumber(maintenance.rollback_history_lines || 0)}`,
                    `disk ${maintenance.disk_status || 'unknown'} ${formatNumber(maintenance.disk_free_gb || 0)} GB · purged dedupe ${formatNumber(maintenance.removed_dedupe_files)}`,
                ] : ['No maintenance status file yet.'],
            },
            {
                title: 'Smoke check',
                status: smoke?.status || 'unavailable',
                lines: smoke ? [
                    formatRelativeTime(smoke.recorded_at),
                    `${smoke.checked_paths?.length || 0} paths checked`,
                    smoke.representative_path || smoke.base_url,
                ] : ['No smoke-check status file yet.'],
            },
            {
                title: 'Latest rollback',
                status: rollback?.status || 'unavailable',
                lines: rollback ? [
                    formatRelativeTime(rollback.recorded_at),
                    `${rollback.service_action || 'unknown'} · ${formatDuration(rollback.duration_seconds || 0)}`,
                    rollback.detail || rollback.target_binary || 'No detail recorded',
                ] : ['No rollback has been recorded yet.'],
            },
            {
                title: 'Telegram delivery',
                status: latestAlert?.delivery_status || 'unavailable',
                lines: latestAlert ? [
                    alertEventTimestamp(latestAlert) ? formatRelativeTime(alertEventTimestamp(latestAlert)) : 'Unknown time',
                    `${latestAlert.source || 'unknown'} · ${latestAlert.kind || 'unknown'}`,
                    `sent ${formatNumber(alertSummary.sent)} · failed ${formatNumber(alertSummary.failed)} · suppressed ${formatNumber(alertSummary.suppressed)}`,
                ] : ['No alert delivery record yet.'],
            },
            {
                title: 'Crawler issue window',
                status: crawler.server_errors_5xx ? 'attention' : 'stable',
                lines: [
                    `${formatNumber(crawler.tracked_hits)} tracked hits`,
                    `404 ${formatNumber(crawler.not_found_404)} · 5xx ${formatNumber(crawler.server_errors_5xx)}`,
                    `Last sent ${alertSummary.last_sent_at ? formatRelativeTime(alertSummary.last_sent_at) : 'never'} · last 404 ${crawler.last_404_at ? formatRelativeTime(crawler.last_404_at) : 'never'}`,
                ],
            },
        ];

        summaryContainer.innerHTML = cards.map((card) => `
            <article class="ops-card">
                <div class="ops-topline">
                    <span class="ops-title">${escapeHtml(card.title)}</span>
                    <span class="ops-status ${escapeAttribute(card.status)}">${escapeHtml(card.status)}</span>
                </div>
                <div class="ops-lines">
                    ${card.lines.map((line) => `<div class="ops-line">${escapeHtml(line)}</div>`).join('')}
                </div>
            </article>
        `).join('');
    }

    if (historyContainer) {
        const items = Array.isArray(operations.rebuild_history) ? operations.rebuild_history : [];
        if (!items.length) {
            historyContainer.innerHTML = '<div class="empty-panel">No rebuild history recorded yet.</div>';
            return;
        }

        historyContainer.innerHTML = items.map((item) => `
            <div class="history-item">
                <div class="history-topline">
                    <span class="history-label">${escapeHtml(item.service_action || 'rebuild')}</span>
                    <span class="ops-status ${escapeAttribute(item.status || 'unknown')}">${escapeHtml(item.status || 'unknown')}</span>
                </div>
                <div class="history-meta">
                    <span data-timestamp="${escapeAttribute(item.recorded_at || '')}">
                        ${escapeHtml(item.recorded_at ? formatRelativeTime(item.recorded_at) : 'Unknown')}
                    </span>
                    · ${escapeHtml(formatDuration(item.duration_seconds || 0))}
                </div>
                <div class="history-detail">${escapeHtml(item.detail || 'No detail recorded')}</div>
            </div>
        `).join('');
    }
}

function renderActivities(activities) {
    const container = document.getElementById('activityFeed');
    if (!container) {
        return;
    }

    if (!activities.length) {
        container.innerHTML = '<div class="empty-panel">No recent activity yet.</div>';
        return;
    }

    container.innerHTML = activities.map((item) => {
        const media = item.thumb_url
            ? `
                <img
                    class="activity-thumb"
                    src="${escapeAttribute(item.thumb_url)}"
                    alt=""
                    loading="lazy"
                >
            `
            : '<div class="activity-thumb-placeholder">OPS</div>';

        return `
            <a class="activity-item" href="${escapeAttribute(item.page_url || '/')}">
                ${media}
                <div>
                    <div class="activity-topline">
                        <span class="activity-kind ${escapeAttribute(item.kind)}">${escapeHtml(item.label)}</span>
                        <span class="activity-time" data-timestamp="${escapeAttribute(item.timestamp)}">
                            ${escapeHtml(formatRelativeTime(item.timestamp))}
                        </span>
                    </div>
                    <div class="activity-detail">${escapeHtml(item.detail)}</div>
                </div>
            </a>
        `;
    }).join('');
}

function renderRecentUploads(items) {
    const container = document.getElementById('recentUploads');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No uploads found.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <a class="upload-card" href="${escapeAttribute(item.page_url)}">
            <img src="${escapeAttribute(item.thumb_url)}" alt="" loading="lazy">
            <div class="upload-title">${escapeHtml(item.title)}</div>
            <div class="upload-tags">${escapeHtml(item.tag_line)}</div>
            <div class="upload-meta">
                ${escapeHtml(formatRelativeTime(item.uploaded_at))} ·
                ${escapeHtml(formatNumber(item.like_count))} likes ·
                ${escapeHtml(formatNumber(item.comment_count))} comments
            </div>
        </a>
    `).join('');
}

function renderCrawlerSummaries(items) {
    const container = document.getElementById('crawlerSummaries');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No crawler telemetry captured yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="crawler-summary-card">
            <div class="crawler-topline">
                <span class="crawler-name">${escapeHtml(item.crawler)}</span>
                <span class="crawler-last-seen" data-timestamp="${escapeAttribute(item.last_seen || '')}">
                    ${escapeHtml(item.last_seen ? formatRelativeTime(item.last_seen) : 'Unknown')}
                </span>
            </div>
            <div class="crawler-stats">
                ${escapeHtml(formatNumber(item.hits))} hits ·
                200 ${escapeHtml(formatNumber(item.ok_200))} ·
                304 ${escapeHtml(formatNumber(item.not_modified_304))} ·
                404 ${escapeHtml(formatNumber(item.not_found_404))} ·
                other ${escapeHtml(formatNumber(item.other_statuses))}
            </div>
        </div>
    `).join('');
}

function renderCrawlerSurfaceSummaries(items) {
    const container = document.getElementById('crawlerSurfaceSummary');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No crawler surface mix yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="crawler-summary-card">
            <div class="crawler-topline">
                <span class="crawler-name">${escapeHtml(item.surface)}</span>
                <span class="crawler-last-seen" data-timestamp="${escapeAttribute(item.last_seen || '')}">
                    ${escapeHtml(item.last_seen ? formatRelativeTime(item.last_seen) : 'Unknown')}
                </span>
            </div>
            <div class="crawler-stats">
                ${escapeHtml(formatNumber(item.hits))} hits ·
                ${escapeHtml(formatNumber(item.unique_paths))} paths ·
                200 ${escapeHtml(formatNumber(item.ok_200))} ·
                304 ${escapeHtml(formatNumber(item.not_modified_304))} ·
                404 ${escapeHtml(formatNumber(item.not_found_404))} ·
                5xx ${escapeHtml(formatNumber(item.server_errors_5xx))}
            </div>
        </div>
    `).join('');
}

function renderCrawlerTopPaths(items) {
    const container = document.getElementById('crawlerTopPaths');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No repeated crawler paths yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="crawler-hit-item">
            <div class="crawler-topline">
                <span class="crawler-name">${escapeHtml(formatNumber(item.hits))} hits</span>
                <span class="crawler-last-seen" data-timestamp="${escapeAttribute(item.last_seen || '')}">
                    ${escapeHtml(item.last_seen ? formatRelativeTime(item.last_seen) : 'Unknown')}
                </span>
            </div>
            <div class="crawler-hit-meta">
                last status ${escapeHtml(formatNumber(item.last_status))}
            </div>
            <div class="crawler-hit-path">${escapeHtml(item.path)}</div>
        </div>
    `).join('');
}

function renderCrawlerHits(items) {
    const container = document.getElementById('crawlerHits');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No recent crawler hits yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="crawler-hit-item">
            <div class="crawler-topline">
                <span class="crawler-name">${escapeHtml(item.crawler)}</span>
                <span class="crawler-last-seen" data-timestamp="${escapeAttribute(item.timestamp)}">
                    ${escapeHtml(formatRelativeTime(item.timestamp))}
                </span>
            </div>
            <div class="crawler-hit-meta">
                ${escapeHtml(item.method)} · status ${escapeHtml(formatNumber(item.status))}
            </div>
            <div class="crawler-hit-path">${escapeHtml(item.path)}</div>
        </div>
    `).join('');
}

function renderUploadPipeline(items) {
    const container = document.getElementById('uploadPipeline');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No recent upload jobs yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => {
        const wrapperTag = item.page_url ? 'a' : 'div';
        const hrefAttribute = item.page_url ? ` href="${escapeAttribute(item.page_url)}"` : '';
        return `
            <${wrapperTag} class="upload-job-item"${hrefAttribute}>
                <div class="job-topline">
                    <span class="job-name">${escapeHtml(item.file_name || item.upload_id)}</span>
                    <span class="job-status ${escapeAttribute(item.state)}">${escapeHtml(item.state)}</span>
                </div>
                <div class="job-meta">
                    phase ${escapeHtml(item.phase)} ·
                    updated <span data-timestamp="${escapeAttribute(item.updated_at)}">${escapeHtml(formatRelativeTime(item.updated_at))}</span>
                </div>
                <div class="job-detail">${escapeHtml(item.message)}</div>
            </${wrapperTag}>
        `;
    }).join('');
}

function renderAlerts(items) {
    const container = document.getElementById('alertList');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No warnings or errors captured.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="alert-item">
            <div class="alert-topline">
                <span class="alert-label">${escapeHtml(item.label)}</span>
                <span class="alert-badge ${escapeAttribute(item.severity)}">${escapeHtml(item.severity)}</span>
            </div>
            <div class="alert-meta">
                <span class="alert-source">${escapeHtml(item.source)}</span> ·
                <span data-timestamp="${escapeAttribute(item.timestamp)}">${escapeHtml(formatRelativeTime(item.timestamp))}</span>
            </div>
            <div class="alert-detail">${escapeHtml(item.detail)}</div>
        </div>
    `).join('');
}

function renderAdminAudit(items) {
    const container = document.getElementById('adminAudit');
    if (!container) {
        return;
    }

    if (!items.length) {
        container.innerHTML = '<div class="empty-panel">No admin audit entries captured yet.</div>';
        return;
    }

    container.innerHTML = items.map((item) => `
        <div class="audit-item">
            <div class="audit-topline">
                <span class="audit-actor">${escapeHtml(item.actor || 'unknown')}</span>
                <span class="ops-status ${escapeAttribute(item.status || 'unknown')}">${escapeHtml(item.status || 'unknown')}</span>
            </div>
            <div class="audit-meta">
                ${escapeHtml(item.action || 'unknown')} ·
                ${escapeHtml(item.source_ip || 'unknown')} ·
                <span data-timestamp="${escapeAttribute(item.timestamp || '')}">
                    ${escapeHtml(item.timestamp ? formatRelativeTime(item.timestamp) : 'Unknown')}
                </span>
            </div>
            <div class="audit-detail">${escapeHtml(item.detail || 'No detail recorded')}</div>
            <div class="audit-path">${escapeHtml(item.request_path || '')}</div>
        </div>
    `).join('');
}

function renderTopTags(tags) {
    const container = document.getElementById('topTags');
    if (!container) {
        return;
    }

    if (!tags.length) {
        container.innerHTML = '<div class="empty-panel">No tag data available.</div>';
        return;
    }

    const peak = Math.max(...tags.map((tag) => tag.count), 1);

    container.innerHTML = tags.map((tag) => {
        const width = clampPercentage((tag.count / peak) * 100);
        return `
            <a class="tag-item" href="${escapeAttribute(tag.url)}">
                <div class="tag-row">
                    <span class="tag-name">${escapeHtml(tag.name)}</span>
                    <span class="tag-count">${escapeHtml(formatNumber(tag.count))}</span>
                </div>
                <div class="tag-bar">
                    <div class="tag-bar-fill" style="width:${width}%"></div>
                </div>
            </a>
        `;
    }).join('');
}

function renderGlobalError(message) {
    const html = `<div class="empty-panel">${escapeHtml(message)}</div>`;
    [
        'metricsGrid',
        'trendChart',
        'storageLegend',
        'systemList',
        'opsSummary',
        'rebuildHistory',
        'activityFeed',
        'recentUploads',
        'crawlerSummaries',
        'crawlerSurfaceSummary',
        'crawlerTopPaths',
        'crawlerHits',
        'uploadPipeline',
        'alertList',
        'adminAudit',
        'topTags',
    ].forEach((id) => {
        const element = document.getElementById(id);
        if (element) {
            element.innerHTML = html;
        }
    });
}

function buildConsoleState(base) {
    return state.streamConnected ? `${base} · stream on` : `${base} · polling only`;
}

function setConsoleState(value) {
    const element = document.getElementById('consoleState');
    if (element) {
        element.textContent = value;
    }
}

function updateRelativeTimes() {
    const lastUpdated = document.getElementById('lastUpdated');
    if (lastUpdated) {
        lastUpdated.textContent = state.lastFetchedAt
            ? formatRelativeTime(state.lastFetchedAt.toISOString())
            : 'Waiting for first snapshot';
    }

    document.querySelectorAll('[data-timestamp]').forEach((element) => {
        const timestamp = element.getAttribute('data-timestamp');
        if (!timestamp) {
            return;
        }
        element.textContent = formatRelativeTime(timestamp);
    });
}

function activityIdentity(item) {
    return `${item.kind}|${item.label}|${item.detail}|${item.page_url || ''}`;
}

function scaledBarHeight(value, peak) {
    if (value <= 0) {
        return 6;
    }
    return Math.max(10, Math.round((value / peak) * 100));
}

function clampPercentage(value) {
    return Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0));
}

function formatNumber(value) {
    return new Intl.NumberFormat('en-US').format(Number(value) || 0);
}

function formatDuration(totalSeconds) {
    const seconds = Number(totalSeconds) || 0;
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);

    if (days > 0) {
        return `${days}d ${hours}h`;
    }
    if (hours > 0) {
        return `${hours}h ${minutes}m`;
    }
    if (minutes > 0) {
        return `${minutes}m`;
    }
    return `${seconds}s`;
}

function formatOpsCounts(counts) {
    if (!counts) {
        return 'No table counts recorded';
    }

    return `users ${formatNumber(counts.users)} · images ${formatNumber(counts.images)} · likes ${formatNumber(counts.likes)} · comments ${formatNumber(counts.comments)}`;
}

function alertEventTimestamp(item) {
    return item?.recorded_at || item?.timestamp || '';
}

function formatRelativeTime(timestamp) {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) {
        return 'Unknown';
    }

    const deltaSeconds = Math.round((date.getTime() - Date.now()) / 1000);
    const absoluteSeconds = Math.abs(deltaSeconds);
    const formatter = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });

    if (absoluteSeconds < 60) {
        return formatter.format(deltaSeconds, 'second');
    }
    if (absoluteSeconds < 3600) {
        return formatter.format(Math.round(deltaSeconds / 60), 'minute');
    }
    if (absoluteSeconds < 86400) {
        return formatter.format(Math.round(deltaSeconds / 3600), 'hour');
    }
    return formatter.format(Math.round(deltaSeconds / 86400), 'day');
}

function formatTimestamp(timestamp) {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) {
        return 'Never';
    }

    return new Intl.DateTimeFormat('en-GB', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        hour12: false,
    }).format(date);
}

function escapeHtml(value) {
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function escapeAttribute(value) {
    return escapeHtml(value);
}
