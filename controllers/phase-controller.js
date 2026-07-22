/** Coordinates phase transitions using the focused gameplay controllers. */
export class PhaseController {
  constructor({
    getState,
    getCampaignVersion,
    getLocalOwner,
    aiController,
    movementController,
    airController,
    combatController,
    productionController,
    incomeFor,
    checkWinner,
    render,
    log,
    pause,
  }) {
    this.getState = getState;
    this.getCampaignVersion = getCampaignVersion;
    this.getLocalOwner = getLocalOwner;
    this.aiController = aiController;
    this.movementController = movementController;
    this.airController = airController;
    this.combatController = combatController;
    this.productionController = productionController;
    this.incomeFor = incomeFor;
    this.checkWinner = checkWinner;
    this.render = render;
    this.log = log;
    this.pause = pause;
  }

  async beginAttackPhase({ includeAi = false, present = true } = {}) {
    const activeCampaign = this.getCampaignVersion();
    const state = this.getState();
    state.resolving = true;
    state.resolutionStep = "movement";
    state.selectedId = null;
    state.selectedIds = [];
    state.intent = null;
    if (includeAi) this.aiController.planDeploymentAndMoves();
    if (present) this.render();

    const moveOrders = state.orders.filter((item) => item.kind === "move");
    const fundedMoveOrders = this.movementController.fundedOrdersForTurn(
      moveOrders,
      state.players,
    );
    const turnOrders = this.movementController.turnOrders(fundedMoveOrders);
    const successfulMoves =
      this.movementController.successfulOrders(turnOrders);
    if (present) await this.pause(180);
    if (activeCampaign !== this.getCampaignVersion()) return;
    if (present) await this.movementController.animate(successfulMoves);
    if (activeCampaign !== this.getCampaignVersion()) return;
    this.movementController.apply(turnOrders, successfulMoves);
    this.movementController.chargeSuccessfulRouteSegments(
      moveOrders,
      successfulMoves,
      state.players,
    );
    const completedMoveIds = this.movementController.advanceRoutes(
      moveOrders,
      successfulMoves,
    );
    state.orders = state.orders.filter(
      (item) => item.kind !== "move" || !completedMoveIds.has(item.id),
    );
    state.phase = "attack";
    state.resolving = false;
    state.resolutionStep = null;
    const movedCount = successfulMoves.filter(
      (order) =>
        !this.getState().units.find(
          (unit) => unit.id === order.unitId && unit.airborne === true,
        ),
    ).length;
    const sortieCount = successfulMoves.length - movedCount;
    this.log(
      sortieCount
        ? `Movement complete. ${sortieCount} bomber sortie${sortieCount === 1 ? " is" : "s are"} airborne; interception and payload release will resolve before normal fire.`
        : movedCount
          ? `Movement complete. ${movedCount} repositioned unit${movedCount === 1 ? " is" : "s are"} unavailable to fire this round.`
          : "Movement complete. Designate attack targets.",
    );
    if (present) this.render();
  }

  async resolveRound({
    includeAi = false,
    random = Math.random,
    present = true,
  } = {}) {
    const activeCampaign = this.getCampaignVersion();
    const state = this.getState();
    state.resolving = true;
    state.resolutionStep = "combat";
    state.selectedId = null;
    state.selectedIds = [];
    state.intent = null;
    try {
      if (includeAi) this.aiController.planAttacks();
      if (present) this.render();
      if (present) await this.pause(480);
      if (activeCampaign !== this.getCampaignVersion()) return;
      await this.airController.resolveSorties(random, { animate: present });
      if (activeCampaign !== this.getCampaignVersion()) return;
      await this.combatController.resolveAttacks(random, { animate: present });
      if (activeCampaign !== this.getCampaignVersion()) return;
      if (present) this.render();
      if (present) await this.pause(360);
      if (activeCampaign !== this.getCampaignVersion()) return;

      this.productionController.completeJobs();
      this.checkWinner();
      if (!state.winner) {
        state.round += 1;
        for (const player of state.players) {
          const income = this.incomeFor(player.id);
          player.resources += income;
          if (player.id === this.getLocalOwner() && income)
            this.log(`Supply network delivered ◆ ${income}.`);
        }
        const survivingUnitIds = new Set(
          state.units.map((unitItem) => unitItem.id),
        );
        state.orders = state.orders.filter(
          (order) =>
            order.kind === "move" && survivingUnitIds.has(order.unitId),
        );
        state.jobs = [];
        for (const unitItem of state.units) unitItem.movedThisRound = false;
        state.phase = "deployment";
        this.log(
          `Round ${String(state.round).padStart(2, "0")} command window opened.`,
        );
      }
    } catch (error) {
      console.error("Combat resolution recovered from an error.", error);
      this.log("Combat resolution interrupted. Command controls restored.", true);
    } finally {
      if (activeCampaign !== this.getCampaignVersion()) return;
      state.resolving = false;
      state.resolutionStep = null;
      if (present) this.render();
    }
  }
}
