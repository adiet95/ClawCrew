import { PageHeader, Card } from '@/components/ui';

export default function Apps() {
  return (
    <div className="flex-1 overflow-auto max-w-7xl mx-auto w-full px-6 py-6 pb-20">
      <PageHeader
        title="Apps"
        description="Install and manage Kiro Crew Apps."
      />
      <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
        <p>App registry projection is under construction.</p>
      </Card>
    </div>
  );
}
