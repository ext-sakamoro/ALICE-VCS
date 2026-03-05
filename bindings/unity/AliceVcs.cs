// ALICE-VCS Unity C# Bindings
// 20 FFI functions for AST tree, diff, commit, and repository
//
// Author: Moroya Sakamoto

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AliceVcs
{
    [StructLayout(LayoutKind.Sequential)]
    public struct AliceVcsDiffStats
    {
        public uint insertCount;
        public uint deleteCount;
        public uint updateCount;
        public uint relabelCount;
        public uint moveCount;
        public uint totalOps;
        public uint patchBytes;
    }

    internal static class Native
    {
        const string DLL = "alice_vcs";

        // AstTree
        [DllImport(DLL)] public static extern IntPtr alice_vcs_tree_create();
        [DllImport(DLL)] public static extern void alice_vcs_tree_destroy(IntPtr handle);
        [DllImport(DLL)] public static extern uint alice_vcs_tree_add_node(IntPtr handle, byte kind, byte[] label, uint parentId);
        [DllImport(DLL)] public static extern uint alice_vcs_tree_add_node_float(IntPtr handle, byte kind, byte[] label, double value, uint parentId);
        [DllImport(DLL)] public static extern uint alice_vcs_tree_node_count(IntPtr handle);
        [DllImport(DLL)] public static extern uint alice_vcs_tree_root_id(IntPtr handle);
        [DllImport(DLL)] public static extern IntPtr alice_vcs_tree_get_label(IntPtr handle, uint nodeId);
        [DllImport(DLL)] public static extern byte alice_vcs_tree_get_kind(IntPtr handle, uint nodeId);
        [DllImport(DLL)] public static extern ulong alice_vcs_tree_subtree_hash(IntPtr handle, uint nodeId);
        [DllImport(DLL)] public static extern void alice_vcs_tree_remove_subtree(IntPtr handle, uint nodeId);

        // Diff
        [DllImport(DLL)] public static extern byte alice_vcs_diff(IntPtr old, IntPtr @new, ref AliceVcsDiffStats stats);

        // Repository
        [DllImport(DLL)] public static extern IntPtr alice_vcs_repo_create();
        [DllImport(DLL)] public static extern void alice_vcs_repo_destroy(IntPtr handle);
        [DllImport(DLL)] public static extern ulong alice_vcs_repo_commit(IntPtr handle, IntPtr tree, byte[] message, byte[] author);
        [DllImport(DLL)] public static extern ulong alice_vcs_repo_head_hash(IntPtr handle);
        [DllImport(DLL)] public static extern uint alice_vcs_repo_commit_count(IntPtr handle);
        [DllImport(DLL)] public static extern byte alice_vcs_repo_create_branch(IntPtr handle, byte[] name);
        [DllImport(DLL)] public static extern byte alice_vcs_repo_checkout(IntPtr handle, byte[] name);

        // Memory
        [DllImport(DLL)] public static extern void alice_vcs_string_free(IntPtr s);

        // Version
        [DllImport(DLL)] public static extern IntPtr alice_vcs_version();
    }

    internal static class Util
    {
        public static byte[] ToNullTerminated(string s)
        {
            var bytes = Encoding.UTF8.GetBytes(s);
            var result = new byte[bytes.Length + 1];
            Array.Copy(bytes, result, bytes.Length);
            return result;
        }

        public static string PtrToString(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero) return null;
            int len = 0;
            while (Marshal.ReadByte(ptr, len) != 0) len++;
            var buf = new byte[len];
            Marshal.Copy(ptr, buf, 0, len);
            return Encoding.UTF8.GetString(buf);
        }
    }

    public enum AstNodeKind : byte
    {
        Root = 0, CsgOp = 1, Primitive = 2, Transform = 3,
        Parameter = 4, Group = 5, Material = 6, Keyframe = 7, Custom = 255
    }

    public class AstTree : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        public AstTree() { _handle = Native.alice_vcs_tree_create(); }
        internal IntPtr Handle => _handle;

        public uint AddNode(AstNodeKind kind, string label, uint parentId)
            => Native.alice_vcs_tree_add_node(_handle, (byte)kind, Util.ToNullTerminated(label), parentId);

        public uint AddNodeFloat(AstNodeKind kind, string label, double value, uint parentId)
            => Native.alice_vcs_tree_add_node_float(_handle, (byte)kind, Util.ToNullTerminated(label), value, parentId);

        public uint NodeCount => Native.alice_vcs_tree_node_count(_handle);
        public uint RootId => Native.alice_vcs_tree_root_id(_handle);

        public string GetLabel(uint nodeId)
        {
            var ptr = Native.alice_vcs_tree_get_label(_handle, nodeId);
            if (ptr == IntPtr.Zero) return null;
            var str = Util.PtrToString(ptr);
            Native.alice_vcs_string_free(ptr);
            return str;
        }

        public AstNodeKind GetKind(uint nodeId)
            => (AstNodeKind)Native.alice_vcs_tree_get_kind(_handle, nodeId);

        public ulong SubtreeHash(uint nodeId)
            => Native.alice_vcs_tree_subtree_hash(_handle, nodeId);

        public void RemoveSubtree(uint nodeId)
            => Native.alice_vcs_tree_remove_subtree(_handle, nodeId);

        public void Dispose()
        {
            if (!_disposed && _handle != IntPtr.Zero)
            {
                Native.alice_vcs_tree_destroy(_handle);
                _handle = IntPtr.Zero;
                _disposed = true;
            }
            GC.SuppressFinalize(this);
        }
        ~AstTree() { Dispose(); }
    }

    public static class Diff
    {
        public static AliceVcsDiffStats Compare(AstTree oldTree, AstTree newTree)
        {
            var stats = new AliceVcsDiffStats();
            Native.alice_vcs_diff(oldTree.Handle, newTree.Handle, ref stats);
            return stats;
        }
    }

    public class Repository : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        public Repository() { _handle = Native.alice_vcs_repo_create(); }

        public ulong Commit(AstTree tree, string message, string author)
            => Native.alice_vcs_repo_commit(_handle, tree.Handle, Util.ToNullTerminated(message), Util.ToNullTerminated(author));

        public ulong HeadHash => Native.alice_vcs_repo_head_hash(_handle);
        public uint CommitCount => Native.alice_vcs_repo_commit_count(_handle);

        public bool CreateBranch(string name)
            => Native.alice_vcs_repo_create_branch(_handle, Util.ToNullTerminated(name)) != 0;

        public bool Checkout(string name)
            => Native.alice_vcs_repo_checkout(_handle, Util.ToNullTerminated(name)) != 0;

        public void Dispose()
        {
            if (!_disposed && _handle != IntPtr.Zero)
            {
                Native.alice_vcs_repo_destroy(_handle);
                _handle = IntPtr.Zero;
                _disposed = true;
            }
            GC.SuppressFinalize(this);
        }
        ~Repository() { Dispose(); }
    }

    public static class Version
    {
        public static string Get() => Util.PtrToString(Native.alice_vcs_version());
    }
}
