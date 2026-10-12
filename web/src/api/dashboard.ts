import client from './client';

export const getOverviewStats = () => client.get('/api/stats/summary');
export const getProxyStatus = () => client.get('/api/proxy/status');
export const getRecentLogs = (limit?: number) =>
  client.get('/api/logs', { params: { size: limit || 10 } });
