import client from './client'

export const exportMigration = () => client.get('/api/migration/export')
export const importMigration = (data: any) => client.post('/api/migration/import', data)
