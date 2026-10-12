import client from './client'

export const listQuotaRules = () => client.get('/api/quota/rules')
export const createQuotaRule = (data: any) => client.post('/api/quota/rules', data)
export const updateQuotaRule = (id: number, data: any) => client.put(`/api/quota/rules/${id}`, data)
export const deleteQuotaRule = (id: number) => client.delete(`/api/quota/rules/${id}`)
export const getQuotaUsage = (params?: any) => client.get('/api/quota/usage', { params })
export const getQuotaGroups = () => client.get('/api/quota/groups')
export const invalidateQuotaCache = () => client.post('/api/quota/cache/invalidate')
