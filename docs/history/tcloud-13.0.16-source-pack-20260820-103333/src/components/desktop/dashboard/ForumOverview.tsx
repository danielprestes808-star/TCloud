import { Folder, Plus } from 'lucide-react';
import type { TCloudForumTree } from '../../../services/tcloudForums';

interface ForumOverviewProps {
  forum: TCloudForumTree;
  onOpenFolder: (folderId: number) => void;
  onWarmFolder?: (folderId: number) => void;
  onCreateFolder: () => void;
}

export function ForumOverview({
  forum,
  onOpenFolder,
  onWarmFolder,
  onCreateFolder,
}: ForumOverviewProps) {
  return (
    <section className="tcloud-forum-overview">
      <header className="tcloud-forum-overview-header">
        <div>
          <div className="flex items-center gap-2">
            <Folder className="h-5 w-5 text-app-accent" />
            <h2>{forum.name}</h2>
          </div>
          <p>{forum.topics.length} {forum.topics.length === 1 ? 'subpasta' : 'subpastas'} aqui.</p>
        </div>
      </header>

      {forum.topics.length === 0 ? (
        <div className="tcloud-forum-overview-empty">
          <Folder className="h-10 w-10" />
          <strong>Nenhuma subpasta aqui</strong>
          <span>Use Nova pasta para organizar seus arquivos como no Explorador do Windows.</span>
          <button type="button" onClick={onCreateFolder}><Plus className="h-4 w-4" /> Nova pasta</button>
        </div>
      ) : (
        <div className="tcloud-forum-overview-grid">
          {forum.topics.map((topic) => (
            <button
              type="button"
              key={topic.id}
              className="tcloud-forum-overview-folder"
              onPointerEnter={() => onWarmFolder?.(topic.id)}
              onFocus={() => onWarmFolder?.(topic.id)}
              onClick={() => onOpenFolder(topic.id)}
            >
              <span className="tcloud-forum-overview-folder-icon">
                <Folder className="h-5 w-5" />
              </span>
              <span>
                <strong>{topic.name}</strong>
                <small>Pasta</small>
              </span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
