package com.cothink.system.core;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Single fractal leaf in the cothink agent loom (ported from Go AgentNode). */
public final class AgentNode {

    public final int id;
    public final int depth;
    public final Map<String, String> metadata;
    private final List<AgentNode> children = new ArrayList<>();

    public AgentNode(int id, int depth) {
        this.id = id;
        this.depth = depth;
        this.metadata = new LinkedHashMap<>();
    }

    public void addChild(AgentNode child) {
        synchronized (children) {
            children.add(child);
        }
    }

    public List<AgentNode> getChildren() {
        synchronized (children) {
            return Collections.unmodifiableList(new ArrayList<>(children));
        }
    }

    public int childCount() {
        synchronized (children) {
            return children.size();
        }
    }
}
