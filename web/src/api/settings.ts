import client from './client'

export const listSettings = () => client.get('/api/settings')
export const getSetting = (key: string) => client.get(`/api/settings/${key}`)
export const upsertSetting = (key: string, data: any) => client.put(`/api/settings/${key}`, data)
export const deleteSetting = (key: string) => client.delete(`/api/settings/${key}`)
