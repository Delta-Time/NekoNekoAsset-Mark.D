// Importer tree subset adapted from AvatarToolbox cb58555726b9a241dd58c7812f9e4058137d74a7.
// Copyright (c) 2024 Yan-K. MIT; see package THIRD_PARTY_NOTICES.md / SOURCE_PROVENANCE.md.
using System;
using System.Collections.Generic;
using NekoNekoAsset.Bridge.SmartPackage;
using UnityEditor;
using UnityEngine;

namespace NekoNekoAsset.Bridge
{
    public sealed partial class ImportReviewWindow
    {
        private const float RowHeight = 18f;
        private const float TreeIndentWidth = 14f;
        private const float TreeFoldoutWidth = 14f;
        private const float TreeToggleWidth = 16f;
        private const float TreeIconWidth = 18f;
        private static GUIStyle rowName;
        private static Texture folderIcon;
        private readonly Dictionary<string, Texture> iconCache = new Dictionary<string, Texture>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<PackageAssetNode, string> treeControlNames = new Dictionary<PackageAssetNode, string>();
        private readonly List<PackageAssetNode> visibleRows = new List<PackageAssetNode>();
        private readonly Dictionary<PackageAssetNode, PackageAssetNode.ToggleState> treeStates = new Dictionary<PackageAssetNode, PackageAssetNode.ToggleState>();
        private int treeStateVersion = -1;
        private bool treeKeyboardFocus;
        // Package trees draw only the rows inside the scroll view, from a flat list rebuilt on Layout.
        private struct FlatRow { public PackageAssetNode Node; public int Depth; }
        private readonly Dictionary<PackageAssetNode, List<FlatRow>> flatTrees = new Dictionary<PackageAssetNode, List<FlatRow>>();
        private readonly Dictionary<PackageAssetNode, float> treeTops = new Dictionary<PackageAssetNode, float>();
        private readonly Dictionary<ImportConflictKind, float> badgeWidths = new Dictionary<ImportConflictKind, float>();
        private float importerViewHeight = 540f;
        private static readonly Color NewColor = new Color(.30f, .70f, .30f, .85f);
        private static readonly Color IdenticalColor = new Color(.25f, .50f, .90f, .85f);
        private static readonly Color UpdateColor = new Color(.80f, .52f, .10f, .90f);
        private static readonly Color ConflictColor = new Color(.85f, .20f, .20f, .90f);

        private PackageAssetNode.ToggleState GetTreeState(PackageAssetNode node)
        {
            if (treeStateVersion != importerCheckVersion)
            {
                treeStates.Clear();
                treeStateVersion = importerCheckVersion;
            }
            if (treeStates.TryGetValue(node, out PackageAssetNode.ToggleState state)) return state;
            state = node.IsChecked ? PackageAssetNode.ToggleState.Checked : PackageAssetNode.ToggleState.Unchecked;
            if (node.IsFolder && node.Children.Count > 0)
            {
                state = GetTreeState(node.Children[0]);
                for (int i = 1; i < node.Children.Count && state != PackageAssetNode.ToggleState.Mixed; i++)
                    if (GetTreeState(node.Children[i]) != state) state = PackageAssetNode.ToggleState.Mixed;
            }
            treeStates[node] = state;
            return state;
        }

        private static GUIStyle RowNameStyle => rowName ?? (rowName = new GUIStyle(EditorStyles.label)
            { alignment = TextAnchor.MiddleLeft, clipping = TextClipping.Clip });

        private string TreeControlName(PackageAssetNode node)
        {
            if (!treeControlNames.TryGetValue(node, out string name))
            {
                name = "NNAImportTree:" + treeControlNames.Count;
                treeControlNames.Add(node, name);
            }
            return name;
        }

        private void BeginTreeGUI()
        {
            Event current = Event.current;
            // A highlighted row scrolled out of view is not drawn, so its focused control has no name.
            string focused = GUI.GetNameOfFocusedControl();
            if (treeKeyboardFocus && (focused.Length == 0 || focused.StartsWith("NNAImportTree:", StringComparison.Ordinal))
                && !EditorGUIUtility.editingTextField && importerHighlightNode != null
                && current.type == EventType.KeyDown)
            {
                int index = visibleRows.IndexOf(importerHighlightNode);
                bool handled = true;
                switch (current.keyCode)
                {
                    case KeyCode.UpArrow:
                        if (index > 0) importerHighlightNode = visibleRows[index - 1];
                        RevealHighlight();
                        break;
                    case KeyCode.DownArrow:
                        if (index >= 0 && index + 1 < visibleRows.Count) importerHighlightNode = visibleRows[index + 1];
                        RevealHighlight();
                        break;
                    case KeyCode.LeftArrow:
                        if (importerHighlightNode.IsFolder && importerHighlightNode.IsExpanded)
                            importerHighlightNode.IsExpanded = false;
                        else
                        {
                            PackageAssetNode parent = DisplayedTreeParent(importerHighlightNode);
                            if (parent != null && !string.IsNullOrEmpty(parent.FullPath)) importerHighlightNode = parent;
                        }
                        break;
                    case KeyCode.RightArrow:
                        if (importerHighlightNode.IsFolder) importerHighlightNode.IsExpanded = true;
                        break;
                    case KeyCode.Space:
                        ClearReviewMessage();
                        importerHighlightNode.SetChecked(GetTreeState(importerHighlightNode) != PackageAssetNode.ToggleState.Checked, true);
                        RefreshImporterConflictPreview(false);
                        break;
                    default: handled = false; break;
                }
                if (handled) { current.Use(); Repaint(); }
            }
            visibleRows.Clear();
        }

        private void RevealHighlight()
        {
            foreach (KeyValuePair<PackageAssetNode, List<FlatRow>> tree in flatTrees)
            {
                int index = tree.Value.FindIndex(row => row.Node == importerHighlightNode);
                if (index < 0 || !treeTops.TryGetValue(tree.Key, out float top)) continue;
                float y = top + index * RowHeight;
                if (y < importerScroll.y) importerScroll.y = y;
                else if (y + RowHeight > importerScroll.y + importerViewHeight) importerScroll.y = y + RowHeight - importerViewHeight;
                return;
            }
        }

        private void DrawVirtualTree(PackageAssetNode root, Dictionary<string, ImportConflict> conflicts)
        {
            if (!flatTrees.TryGetValue(root, out List<FlatRow> rows) || Event.current.type == EventType.Layout)
            {
                if (rows == null) flatTrees[root] = rows = new List<FlatRow>();
                rows.Clear();
                Flatten(root, 0, rows);
            }
            // Keyboard order includes the rows outside the view.
            foreach (FlatRow row in rows) visibleRows.Add(row.Node);
            Rect block = GUILayoutUtility.GetRect(0f, rows.Count * RowHeight, GUILayout.ExpandWidth(true));
            if (Event.current.type == EventType.Repaint) treeTops[root] = block.y;
            // The last repaint's position keeps the drawn slice identical across a frame's events.
            float top = treeTops.TryGetValue(root, out float known) ? known : 0f;
            int first = Mathf.Max(0, Mathf.FloorToInt((importerScroll.y - top) / RowHeight) - 1);
            int last = Mathf.Min(rows.Count, Mathf.CeilToInt((importerScroll.y + importerViewHeight - top) / RowHeight) + 1);
            for (int i = first; i < last; i++)
                DrawNodeRow(rows[i].Node, rows[i].Depth, conflicts, new Rect(block.x, block.y + i * RowHeight, block.width, RowHeight));
        }

        private static void Flatten(PackageAssetNode node, int depth, List<FlatRow> rows)
        {
            // The synthetic Project container is not a selectable package entry.
            if (node.Parent == null && string.IsNullOrEmpty(node.FullPath))
            {
                foreach (PackageAssetNode topLevel in node.Children) Flatten(topLevel, depth, rows);
                return;
            }
            rows.Add(new FlatRow { Node = node, Depth = depth });
            if (node.IsFolder && node.IsExpanded)
                foreach (PackageAssetNode child in node.Children) Flatten(child, depth + 1, rows);
        }

        private PackageAssetNode DisplayedTreeParent(PackageAssetNode node)
            => looseTreeOwners.ContainsKey(node) ? (individualDestinations ? null : unifiedLooseLeafParent) : node.Parent;

        private Rect DrawRowCore(PackageAssetNode node, int depth, float tailWidth, Rect? placed = null)
        {
            Rect row;
            if (placed.HasValue) row = placed.Value;
            else
            {
                row = GUILayoutUtility.GetRect(0f, RowHeight, GUILayout.Height(RowHeight), GUILayout.ExpandWidth(true));
                visibleRows.Add(node);
            }
            if (node == importerHighlightNode && Event.current.type == EventType.Repaint)
                EditorGUI.DrawRect(row, new Color(.24f, .48f, .90f, .20f));
            float middleY = row.y + (RowHeight - 16f) * .5f;
            float controlsWidth = TreeFoldoutWidth + TreeToggleWidth + TreeIconWidth + 4f;
            // Keep essential controls and the conflict tag inside narrow windows, even for deep trees.
            float indent = Mathf.Min(depth * TreeIndentWidth, Mathf.Max(0f, row.width - tailWidth - controlsWidth - 32f));
            float x = row.x + indent;
            var foldoutRect = new Rect(x, middleY, TreeFoldoutWidth, 16f);
            if (node.IsFolder && node.Children.Count > 0)
            {
                bool expanded = EditorGUI.Foldout(foldoutRect, node.IsExpanded, GUIContent.none, true);
                // Keys then act on the row whose arrow was clicked, never on an earlier highlighted row.
                if (expanded != node.IsExpanded) { node.IsExpanded = expanded; HighlightRow(node); }
            }
            x += TreeFoldoutWidth;

            var toggleRect = new Rect(x, middleY, TreeToggleWidth, 16f);
            PackageAssetNode.ToggleState state = GetTreeState(node);
            bool previousMixed = EditorGUI.showMixedValue;
            EditorGUI.showMixedValue = state == PackageAssetNode.ToggleState.Mixed;
            bool wasChecked = state == PackageAssetNode.ToggleState.Checked;
            bool next;
            try
            {
                GUI.SetNextControlName(TreeControlName(node));
                next = EditorGUI.Toggle(toggleRect, wasChecked);
            }
            finally { EditorGUI.showMixedValue = previousMixed; }
            if (next != wasChecked)
            {
                ClearReviewMessage();
                importerHighlightNode = node;
                treeKeyboardFocus = true;
                node.SetChecked(next, true);
                RefreshImporterConflictPreview(false);
            }
            x += TreeToggleWidth + 2f;
            if (Event.current.type == EventType.Repaint)
            {
                Texture icon = GetCachedIconFor(node);
                if (icon != null) GUI.DrawTexture(new Rect(x, middleY, TreeIconWidth, 16f), icon, ScaleMode.ScaleToFit);
            }
            x += TreeIconWidth + 2f;
            float nameRight = Mathf.Max(x, row.xMax - tailWidth - 2f);
            var nameRect = new Rect(x, row.y, Mathf.Max(0f, nameRight - x), RowHeight);
            string tooltip = node.FullPath;
            if (looseTreeOwners.TryGetValue(node, out LoadedPackage owner) && !string.IsNullOrEmpty(owner.DisplayName))
                tooltip = owner.DisplayName + "\n" + tooltip;
            GUI.Label(nameRect, new GUIContent(node.Name, tooltip), RowNameStyle);
            if (Event.current.type == EventType.MouseDown && Event.current.button == 0 && nameRect.Contains(Event.current.mousePosition))
            {
                HighlightRow(node);
                Event.current.Use();
            }
            return new Rect(row.xMax - tailWidth, row.y, tailWidth, RowHeight);
        }

        private void HighlightRow(PackageAssetNode node)
        {
            importerHighlightNode = node;
            treeKeyboardFocus = true;
            GUI.FocusControl(TreeControlName(node));
            Repaint();
        }

        private Texture GetCachedIconFor(PackageAssetNode node)
        {
            string path = node.FullPath;
            if (string.IsNullOrEmpty(path)) return ResolveFallbackIcon(node);
            if (iconCache.TryGetValue(path, out Texture cached)) return cached;
            Texture icon = AssetDatabase.GetCachedIcon(path);
            if (icon == null) icon = ResolveFallbackIcon(node);
            iconCache[path] = icon;
            return icon;
        }

        private static Texture ResolveFallbackIcon(PackageAssetNode node)
        {
            if (node.IsFolder)
            {
                if (folderIcon == null)
                {
                    GUIContent content = EditorGUIUtility.IconContent("Folder Icon");
                    folderIcon = content != null ? content.image : null;
                    if (folderIcon == null) folderIcon = AssetDatabase.GetCachedIcon("Assets");
                }
                return folderIcon;
            }
            Texture icon = string.IsNullOrEmpty(node.Name) ? null : UnityEditorInternal.InternalEditorUtility.GetIconForFile(node.Name);
            return icon != null ? icon : AssetPreview.GetMiniTypeThumbnail(typeof(UnityEngine.Object));
        }

        private void DrawNode(PackageAssetNode node, int depth, Dictionary<string, ImportConflict> conflicts)
        {
            if (node == null) return;
            if (looseTreeOwners.TryGetValue(node, out LoadedPackage owner)) conflicts = owner.ConflictByPath;
            // The synthetic Project container is not a selectable package entry.
            if (node.Parent == null && string.IsNullOrEmpty(node.FullPath))
            {
                foreach (PackageAssetNode topLevel in node.Children) DrawNode(topLevel, depth, conflicts);
                return;
            }
            DrawNodeRow(node, depth, conflicts);
            if (node.IsFolder && node.IsExpanded)
                foreach (PackageAssetNode child in node.Children) DrawNode(child, depth + 1, conflicts);
        }

        private void DrawNodeRow(PackageAssetNode node, int depth, Dictionary<string, ImportConflict> conflicts, Rect? placed = null)
        {
            ImportConflictKind badgeKind = default;
            bool hasBadge = true;
            // A folder shows what most needs a look among its checked contents.
            if (node.IsFolder)
            {
                if (node.HasCheckedBlocked) badgeKind = ImportConflictKind.Blocked;
                else if (node.HasCheckedPathConflict) badgeKind = ImportConflictKind.PathConflict;
                else if (node.HasCheckedGuidConflict) badgeKind = ImportConflictKind.GuidConflict;
                else if (node.HasCheckedUpdate) badgeKind = ImportConflictKind.Update;
                else if (node.HasCheckedDuplicate) badgeKind = ImportConflictKind.Duplicate;
                else hasBadge = false;
            }
            else if (node.IsChecked && conflicts != null && conflicts.TryGetValue(node.FullPath, out ImportConflict conflict)
                && !(conflict.Kind == ImportConflictKind.Duplicate && conflict.ExistingFromProject))
                badgeKind = conflict.Kind;
            else hasBadge = false;
            float badgeWidth = hasBadge ? BadgeWidth(badgeKind) : 0f;
            Rect tail = DrawRowCore(node, depth, badgeWidth, placed);
            if (hasBadge) DrawConflictBadge(new Rect(tail.x, tail.y + (RowHeight - 16f) * .5f, badgeWidth, 16f), badgeKind);
        }

        private float BadgeWidth(ImportConflictKind kind)
        {
            if (!badgeWidths.TryGetValue(kind, out float width))
                badgeWidths[kind] = width = EditorStyles.miniLabel.CalcSize(new GUIContent(localization.Kind(kind))).x + 12f;
            return width;
        }

        private static Color BadgeColor(ImportConflictKind kind)
            => kind == ImportConflictKind.New ? NewColor : kind == ImportConflictKind.Duplicate ? IdenticalColor
                : kind == ImportConflictKind.Update ? UpdateColor : ConflictColor;

        private void DrawConflictBadge(Rect rect, ImportConflictKind kind)
        {
            if (Event.current.type != EventType.Repaint) return;
            EditorGUI.DrawRect(rect, BadgeColor(kind));
            GUI.Label(rect, localization.Kind(kind), EditorStyles.whiteMiniLabel);
        }
    }
}
