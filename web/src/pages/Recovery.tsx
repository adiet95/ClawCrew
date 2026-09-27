import { PageHeader, Card } from '@/components/ui';

export default function Recovery() {
  return (
    <div className="flex-1 overflow-auto max-w-7xl mx-auto w-full px-6 py-6 pb-20">
      <PageHeader
        title="Recovery Console"
        description="Unified recovery console for lost or stuck tasks."
      />
      <Card className="p-6 text-center text-sm" style={{ color: 'var(--pc-text-faint)' }}>
        <p>Recovery console projection is under construction.</p>
      </Card>
    </div>
  );
}
