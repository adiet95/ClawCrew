import { useState, useEffect } from 'react';
import { PageHeader, Card, Button } from '@/components/ui';

interface McpServerDefinition {
  command: string;
  args: string[];
  env: Record<string, string>;
}

interface AppManifest {
  id: string;
  name: string;
  version: string;
  mcp_server?: McpServerDefinition;
}

export default function Apps() {
  const [apps, setApps] = useState<AppManifest[]>([]);

  useEffect(() => {
    // Mock data for MCP server as App
    setApps([
      {
        id: 'mcp-github',
        name: 'GitHub MCP',
        version: '1.0.0',
        mcp_server: {
          command: 'npx',
          args: ['-y', '@modelcontextprotocol/server-github'],
          env: {}
        }
      }
    ]);
  }, []);

  return (
    <div className="flex-1 overflow-auto max-w-7xl mx-auto w-full px-6 py-6 pb-20">
      <PageHeader
        title="Apps"
        description="Install and manage ClawCrew Apps."
      />
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 mt-6">
        {apps.map((app) => (
          <Card key={app.id} className="p-6">
            <h3 className="text-lg font-semibold">{app.name}</h3>
            <p className="text-sm text-gray-500 mb-4">v{app.version}</p>
            {app.mcp_server && (
              <div className="mb-4 text-xs bg-gray-100 p-2 rounded">
                <span className="font-semibold">MCP Server</span>
                <div>Command: {app.mcp_server.command} {app.mcp_server.args.join(' ')}</div>
              </div>
            )}
            <Button onClick={() => alert(`Launching ${app.name}...`)}>Launch</Button>
          </Card>
        ))}
      </div>
    </div>
  );
}
